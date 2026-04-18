/// Like vec! but automatically wraps string arguments in a Cow.
/// Handles both borrowed and owned data appropriately.
macro_rules! args {
  ($($arg:expr),* $(,)?) => {
      vec![$(std::borrow::Cow::<'static, str>::from($arg)),*]
  };
}
