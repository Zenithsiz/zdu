//! Stats

/// File stats
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
#[derive(derive_more::Add, derive_more::AddAssign, derive_more::Sum)]
pub struct Stats {
	pub files:  u64,
	pub size:   u64,
	pub blocks: u64,
}

/// File stats diff
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
#[derive(derive_more::Add, derive_more::AddAssign, derive_more::Sum)]
pub struct StatsDiff {
	pub add:    Stats,
	pub remove: Stats,
}
