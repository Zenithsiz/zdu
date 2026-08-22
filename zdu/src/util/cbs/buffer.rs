//! Buffer trait

use {
	super::CbsIdx,
	core::{mem, ops::RangeBounds},
	std::{
		ffi::{OsStr, OsString},
		ops,
		os::unix::prelude::{OsStrExt, OsStringExt},
	},
};

/// Buffer type
pub trait Buffer: Default {
	type Value;
	type Slice: ?Sized;
	type Drain<'a>
	where
		Self: 'a;

	fn len(&self) -> usize;
	fn truncate(&mut self, len: usize);
	fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> Self::Drain<'_>;
	fn push(&mut self, value: Self::Value);
	fn insert(&mut self, idx: usize, value: Self::Value);
	fn extend_from_slice(&mut self, slice: &Self::Slice)
	where
		Self::Value: Clone;
	fn slice<R: RangeBounds<usize>>(&self, range: R) -> &Self::Slice;
	fn get(&self, idx: usize) -> &Self::Value;
}

pub trait BufferAccessMut: Buffer {
	fn slice_mut<R: RangeBounds<usize>>(&mut self, range: R) -> &mut Self::Slice;
	fn get_mut(&mut self, idx: usize) -> &mut Self::Value;
	fn get_disjoint_mut<const N: usize>(&mut self, idxs: [CbsIdx; N]) -> [&mut Self::Value; N];
}

impl<T> Buffer for Vec<T> {
	type Drain<'a>
		= std::vec::Drain<'a, T>
	where
		Self: 'a;
	type Slice = [T];
	type Value = T;

	impl_buffer_common! {}

	fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> Self::Drain<'_> {
		self.drain(range)
	}

	fn push(&mut self, value: Self::Value) {
		self.push(value);
	}

	fn insert(&mut self, idx: usize, value: Self::Value) {
		self.insert(idx, value);
	}

	fn extend_from_slice(&mut self, slice: &Self::Slice)
	where
		Self::Value: Clone,
	{
		self.extend_from_slice(slice);
	}

	fn slice<R: RangeBounds<usize>>(&self, range: R) -> &Self::Slice {
		&self[(range.start_bound().copied(), range.end_bound().copied())]
	}

	fn get(&self, idx: usize) -> &Self::Value {
		&self[idx]
	}
}

impl<T> BufferAccessMut for Vec<T> {
	fn slice_mut<R: RangeBounds<usize>>(&mut self, range: R) -> &mut Self::Slice {
		&mut self[(range.start_bound().copied(), range.end_bound().copied())]
	}

	fn get_mut(&mut self, idx: usize) -> &mut Self::Value {
		&mut self[idx]
	}

	fn get_disjoint_mut<const N: usize>(&mut self, idxs: [CbsIdx; N]) -> [&mut Self::Value; N] {
		(**self)
			.get_disjoint_mut(idxs.map(|idx| idx.0))
			.expect("Cannot index multiple non-disjoint values at once")
	}
}

impl Buffer for OsString {
	type Drain<'a>
		= ()
	where
		Self: 'a;
	type Slice = OsStr;
	type Value = u8;

	impl_buffer_common! {}

	fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> Self::Drain<'_> {
		assert_eq!(range.end_bound().copied(), ops::Bound::Unbounded);
		let ops::Bound::Included(start_idx) = range.start_bound().copied() else {
			panic!("Expected an included start bound");
		};
		self.truncate(start_idx);
	}

	fn push(&mut self, value: Self::Value) {
		self.push(OsStr::from_bytes(&[value]));
	}

	fn insert(&mut self, idx: usize, value: Self::Value) {
		let mut bytes = mem::take(self).into_vec();
		bytes.insert(idx, value);
		*self = Self::from_vec(bytes);
	}

	fn extend_from_slice(&mut self, slice: &Self::Slice)
	where
		Self::Value: Clone,
	{
		self.push(slice);
	}

	fn slice<R: RangeBounds<usize>>(&self, range: R) -> &Self::Slice {
		self.slice_encoded_bytes(range)
	}

	fn get(&self, idx: usize) -> &Self::Value {
		&self.as_bytes()[idx]
	}
}

macro impl_buffer_common() {
	fn len(&self) -> usize {
		(**self).len()
	}

	fn truncate(&mut self, len: usize) {
		self.truncate(len);
	}
}
