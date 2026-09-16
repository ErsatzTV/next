// Left to itself, the AMF runtime picks the iGPU on an APU + dGPU machine. ffmpeg can
// only target an adapter by deriving AMF from a `d3d11va` device, so the probe creates
// its own D3D11 device on the same adapter index and reports capabilities for that GPU.

use libd3d11_sys::{AdapterInfo, D3d11Device, Dx11Lib, VENDOR_ID_AMD};

use crate::capabilities::amf::{AmfAdapter, AmfDeviceTarget};
use crate::capabilities::vulkan;
use crate::error::FFPipelineError;

pub(super) struct SelectedAdapter {
    pub info: AmfAdapter,
    pub device: D3d11Device,
}

fn error(message: String) -> FFPipelineError {
    FFPipelineError::AmfCapabilitiesError(message)
}

/// `Ok(None)` falls back to the runtime's own choice, which is also what ffmpeg does
/// without a derived device, so probe and ffmpeg still agree. An explicit index that
/// cannot be used is an error: falling back would silently move to another GPU.
pub(super) fn select(target: AmfDeviceTarget) -> Result<Option<SelectedAdapter>, FFPipelineError> {
    let explicit = matches!(target, AmfDeviceTarget::Adapter(_));

    let dx = match Dx11Lib::load() {
        Ok(dx) => dx,
        Err(e) if explicit => {
            return Err(error(format!(
                "{target:?} requested but DXGI is unavailable: {e}"
            )));
        }
        Err(e) => {
            log::debug!("[amf] DXGI unavailable, letting the runtime pick the adapter: {e}");
            return Ok(None);
        }
    };

    let adapters = match dx.enumerate_adapters() {
        Ok(adapters) => adapters,
        Err(e) if explicit => return Err(error(format!("{target:?} requested but {e}"))),
        Err(e) => {
            log::debug!("[amf] failed to enumerate adapters, letting the runtime pick: {e}");
            return Ok(None);
        }
    };

    for adapter in &adapters {
        log::debug!(
            "[amf] adapter {}: \"{}\" {:04x}:{:04x}, {} MiB dedicated{}",
            adapter.index,
            adapter.description,
            adapter.vendor_id,
            adapter.device_id,
            adapter.dedicated_video_memory >> 20,
            if adapter.is_software() {
                ", software"
            } else {
                ""
            }
        );
    }

    let chosen = match target {
        AmfDeviceTarget::Adapter(index) => {
            let adapter = adapters.iter().find(|a| a.index == index).ok_or_else(|| {
                error(format!(
                    "amf_device {index} not found ({} adapters present)",
                    adapters.len()
                ))
            })?;
            if adapter.vendor_id != VENDOR_ID_AMD {
                log::warn!(
                    "[amf] amf_device {index} (\"{}\") is not an AMD adapter",
                    adapter.description
                );
            }
            adapter
        }
        AmfDeviceTarget::Auto => {
            let luid_hint = || match vulkan::best_device_luid(VENDOR_ID_AMD) {
                Ok(luid) => luid,
                Err(e) => {
                    log::debug!("[amf] no Vulkan hint for the preferred adapter: {e}");
                    None
                }
            };
            match choose(&adapters, luid_hint) {
                Some(adapter) => adapter,
                None => {
                    log::debug!("[amf] no AMD adapter found, letting the runtime pick");
                    return Ok(None);
                }
            }
        }
    };

    let device = match dx.create_device(chosen.index) {
        Ok(device) => device,
        Err(e) if explicit => return Err(error(e)),
        Err(e) => {
            log::warn!(
                "[amf] {e}; letting the runtime pick the adapter instead of \"{}\"",
                chosen.description
            );
            return Ok(None);
        }
    };

    log::debug!(
        "[amf] selected adapter {}: \"{}\"",
        chosen.index,
        chosen.description
    );

    Ok(Some(SelectedAdapter {
        info: AmfAdapter {
            index: chosen.index,
            description: chosen.description.clone(),
            vendor_id: chosen.vendor_id,
            device_id: chosen.device_id,
        },
        device,
    }))
}

/// Dedicated memory is the fallback rule because an iGPU reserves far less than any
/// discrete card.
fn choose(
    adapters: &[AdapterInfo],
    luid_hint: impl FnOnce() -> Option<[u8; 8]>,
) -> Option<&AdapterInfo> {
    let amd: Vec<&AdapterInfo> = adapters
        .iter()
        .filter(|a| a.vendor_id == VENDOR_ID_AMD && !a.is_software())
        .collect();

    match amd.as_slice() {
        [] => None,
        [only] => Some(only),
        _ => {
            if let Some(luid) = luid_hint()
                && let Some(matched) = amd.iter().find(|a| a.luid == luid)
            {
                log::debug!("[amf] adapter {} matches the Vulkan device", matched.index);
                return Some(matched);
            }
            log::debug!("[amf] no Vulkan match, preferring the most dedicated memory");
            amd.into_iter().max_by_key(|a| a.dedicated_video_memory)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(index: u32, vendor_id: u32, vram_mib: usize, luid: u8, flags: u32) -> AdapterInfo {
        AdapterInfo {
            index,
            description: format!("adapter {index}"),
            vendor_id,
            device_id: index,
            dedicated_video_memory: vram_mib << 20,
            luid: [luid; 8],
            flags,
        }
    }

    const IGPU: u32 = 0;
    const DGPU: u32 = 1;

    fn laptop() -> Vec<AdapterInfo> {
        vec![
            adapter(IGPU, VENDOR_ID_AMD, 512, 1, 0),
            adapter(DGPU, VENDOR_ID_AMD, 8192, 2, 0),
            adapter(2, 0x1414, 0, 3, libd3d11_sys::DXGI_ADAPTER_FLAG_SOFTWARE),
        ]
    }

    #[test]
    fn vulkan_luid_wins_when_it_matches() {
        let adapters = laptop();
        assert_eq!(
            choose(&adapters, || Some([1; 8])).map(|a| a.index),
            Some(IGPU)
        );
        assert_eq!(
            choose(&adapters, || Some([2; 8])).map(|a| a.index),
            Some(DGPU)
        );
    }

    #[test]
    fn falls_back_to_dedicated_memory() {
        let adapters = laptop();
        assert_eq!(choose(&adapters, || None).map(|a| a.index), Some(DGPU));
        assert_eq!(
            choose(&adapters, || Some([9; 8])).map(|a| a.index),
            Some(DGPU)
        );
    }

    #[test]
    fn single_amd_adapter_needs_no_hint() {
        let adapters = vec![
            adapter(0, 0x10de, 16384, 1, 0),
            adapter(1, VENDOR_ID_AMD, 512, 2, 0),
        ];
        let chosen = choose(&adapters, || panic!("hint should not be consulted"));
        assert_eq!(chosen.map(|a| a.index), Some(1));
    }

    #[test]
    fn ignores_non_amd_and_software_adapters() {
        let adapters = vec![
            adapter(0, 0x10de, 16384, 1, 0),
            adapter(1, 0x1414, 0, 2, libd3d11_sys::DXGI_ADAPTER_FLAG_SOFTWARE),
        ];
        assert!(choose(&adapters, || None).is_none());

        let software_amd = vec![adapter(
            0,
            VENDOR_ID_AMD,
            0,
            1,
            libd3d11_sys::DXGI_ADAPTER_FLAG_SOFTWARE,
        )];
        assert!(choose(&software_amd, || None).is_none());
    }
}
