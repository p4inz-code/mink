/// Opaque identifier for a source file registered in a [`SourceMap`](crate::source::SourceMap).
///
/// Ids are assigned sequentially by the map that creates them and remain
/// stable for the lifetime of that map. They are cheap to copy and compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(u32);

impl SourceId {
    /// Creates a new id from its raw numeric value.
    ///
    /// Ids should normally be produced by a `SourceMap`; constructing one
    /// directly is only useful for tests and tooling that manages files
    /// itself.
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }
}
