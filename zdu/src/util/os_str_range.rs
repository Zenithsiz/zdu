//! [`OsStr`] range

use {
	core::{ops, range},
	std::ffi::{OsStr, OsString},
};

/// [`OsStr`] range
#[derive(Clone, Copy, Debug)]
pub struct OsStrRange(pub range::Range<usize>);

impl From<ops::Range<usize>> for OsStrRange {
	fn from(range: ops::Range<usize>) -> Self {
		Self(range.into())
	}
}

impl ops::Index<OsStrRange> for OsStr {
	type Output = OsStr;

	fn index(&self, range: OsStrRange) -> &Self::Output {
		self.slice_encoded_bytes(range.0.start..range.0.end)
	}
}

impl ops::Index<OsStrRange> for OsString {
	type Output = OsStr;

	fn index(&self, range: OsStrRange) -> &Self::Output {
		(**self).index(range)
	}
}
