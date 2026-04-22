/// Like vec! but automatically wraps string arguments in a Cow.
/// Handles both borrowed and owned data appropriately.
macro_rules! args {
  ($($arg:expr),* $(,)?) => {
      vec![$(std::borrow::Cow::<'static, str>::from($arg)),*]
  };
}
macro_rules! gen_subset {
    ($name:ident, $base:ident, $($variant:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name($base);

        #[allow(non_upper_case_globals)]
        impl $name {
            $(pub const $variant: Self = Self($base::$variant);)*
        }

        impl From<$name> for $base {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = $base;
            fn deref(&self) -> &$base {
                &self.0
            }
        }
    };
}
