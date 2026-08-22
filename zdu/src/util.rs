//! Utilities

pub mod colors;
pub mod os_str_range;
pub mod var_int;

pub use self::{colors::Colorize, os_str_range::OsStrRange};

use {
	app_error::Context,
	core::{fmt, iter, mem},
	humansize::ISizeFormatter,
	std::{
		ffi::{OsStr, OsString},
		fs,
		io::prelude::Read,
		os::unix::prelude::OsStrExt,
		path::{Path, PathBuf},
		sync::LazyLock,
	},
};

/// Error type
pub type AppError = app_error::AppError<()>;

/// Formats a size
pub fn fmt_size<T: humansize::ToF64>(value: T) -> ISizeFormatter<T, humansize::FormatSizeOptions> {
	ISizeFormatter::new(value, humansize::BINARY)
}

/// Computes the md5 of a file by path
pub fn file_md5(path: &Path) -> Result<md5::Digest, AppError> {
	let mut file = fs::File::open(path).context("Unable to open path")?;

	let mut buf = [0; 4096];
	let mut digest = md5::Context::new();
	while let len = file.read(&mut buf).context("Unable to read file")? &&
		len > 0
	{
		digest.consume(&buf[..len]);
	}

	Ok(digest.finalize())
}

/// Returns the md5 of an empty file
pub fn empty_md5() -> md5::Digest {
	static EMPTY_DIGEST: LazyLock<md5::Digest> = LazyLock::new(|| md5::compute([]));
	*EMPTY_DIGEST
}

#[extend::ext(name = OsStrStrip)]
pub impl OsStr {
	/// Strips the prefix `prefix` from this string
	fn strip_prefix(&self, prefix: impl AsRef<OsStr>) -> Option<&OsStr> {
		let prefix = self
			.as_encoded_bytes()
			.strip_prefix(prefix.as_ref().as_encoded_bytes())?;

		Some(OsStr::from_bytes(prefix))
	}

	/// Strips the suffix `suffix` from this string
	fn strip_suffix(&self, suffix: impl AsRef<OsStr>) -> Option<&OsStr> {
		let suffix = self
			.as_encoded_bytes()
			.strip_suffix(suffix.as_ref().as_encoded_bytes())?;

		Some(OsStr::from_bytes(suffix))
	}
}

/// Gets the common prefix of two os strings and their non-common parts.
///
/// If no prefix exists, returns `("", lhs, rhs)`.
/// If both strings are equal, returns `(lhs, "", "")`
pub fn common_os_str_prefix<'a>(lhs: &'a OsStr, rhs: &'a OsStr) -> (&'a OsStr, &'a OsStr, &'a OsStr) {
	let lhs_bytes = lhs.as_encoded_bytes();
	let rhs_bytes = rhs.as_encoded_bytes();
	let idx = iter::zip(lhs_bytes, rhs_bytes)
		.position(|(lhs, rhs)| lhs != rhs)
		.unwrap_or(usize::min(lhs_bytes.len(), rhs_bytes.len()));

	(
		lhs.slice_encoded_bytes(..idx),
		lhs.slice_encoded_bytes(idx..),
		rhs.slice_encoded_bytes(idx..),
	)
}

/// Gets the common suffix of two os strings and their non-common parts.
///
/// If no prefix exists, returns `(lhs, rhs, "")`.
/// If both strings are equal, returns `("", "", lhs)`
pub fn common_os_str_suffix<'a>(lhs: &'a OsStr, rhs: &'a OsStr) -> (&'a OsStr, &'a OsStr, &'a OsStr) {
	let lhs_bytes = lhs.as_encoded_bytes();
	let rhs_bytes = rhs.as_encoded_bytes();
	let (lhs_idx, rhs_idx) =
		match iter::zip(lhs_bytes.iter().rev(), rhs_bytes.iter().rev()).position(|(lhs, rhs)| lhs != rhs) {
			Some(idx) => (lhs_bytes.len() - idx, rhs_bytes.len() - idx),
			None => {
				let min = usize::min(lhs_bytes.len(), rhs_bytes.len());
				(lhs_bytes.len() - min, rhs_bytes.len() - min)
			},
		};

	(
		lhs.slice_encoded_bytes(..lhs_idx),
		rhs.slice_encoded_bytes(..rhs_idx),
		lhs.slice_encoded_bytes(lhs_idx..),
	)
}

#[extend::ext(name = OsStrAsPath)]
pub impl OsStr {
	/// Gets this byte slice as a path
	fn as_path(&self) -> &Path {
		Path::new(self)
	}
}

#[extend::ext(name = OsStringPath)]
pub impl OsString {
	/// Extends this vector of bytes with a path, similarly to [`PathBuf::push`]
	fn push_path(&mut self, path: impl AsRef<Path>) {
		let path_buf = mem::take(self);
		let mut path_buf = PathBuf::from(path_buf);
		path_buf.push(path);
		*self = path_buf.into_os_string();
	}
}

#[extend::ext(name = BytesDisplayHex)]
pub impl [u8] {
	/// Displays these bytes as a hex string
	fn display_hex(&self) -> impl fmt::Display {
		fmt::from_fn(move |f| {
			for byte in self {
				f.write_fmt(format_args!("{byte:02x}"))?;
			}

			Ok(())
		})
	}
}
