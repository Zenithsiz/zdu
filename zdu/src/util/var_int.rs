//! Variable-length integers

use std::io;

/// Encodes a variable-length `u64`.
///
/// Returns the number of bytes written
pub fn encode<W: io::Write>(writer: &mut W, mut n: u64) -> Result<u64, io::Error> {
	let mut bytes_written = 0;
	loop {
		match n <= 0x7f || bytes_written + 1 >= MAX_BUFFER_LENGTH {
			true => {
				let part = n.checked_truncate().expect("Should fit within a single byte");
				writer.write_all(&[part])?;
				bytes_written += 1;
				break;
			},
			false => {
				let part = (n.truncate::<u8>() & 0x7f) | 0x80;
				writer.write_all(&[part])?;
				bytes_written += 1;
				n >>= 7;
			},
		}
	}

	Ok(bytes_written)
}

/// Decodes a variable-length `u64`
pub fn decode<W: io::Read>(reader: &mut W) -> Result<u64, io::Error> {
	let mut bytes_read = 0;
	let mut n: u64 = 0;
	loop {
		let [part] = reader.read_array()?;
		match part & 0x80 == 0 || bytes_read + 1 >= MAX_BUFFER_LENGTH {
			true => {
				n |= part.widen::<u64>() << (7 * bytes_read);
				break;
			},
			false => n |= (part & 0x7f).widen::<u64>() << (7 * bytes_read),
		}

		bytes_read += 1;
	}

	Ok(n)
}

const MAX_BUFFER_LENGTH: u64 = 9;

#[cfg(test)]
mod tests {
	use {
		crate::util::{AppError, BytesDisplayHex},
		app_error::{Context, ensure},
		std::io,
	};

	#[test]
	fn encode_decode() {
		let cases = [
			(0x00, b"\x00".as_slice()),
			(0x01, b"\x01"),
			(0x02, b"\x02"),
			(0x7f, b"\x7f"),
			(0x80, b"\x80\x01"),
			(0x81, b"\x81\x01"),
			(0xfe, b"\xfe\x01"),
			(0xff, b"\xff\x01"),
			(0x3fff, b"\xff\x7f"),
			(0x4000, b"\x80\x80\x01"),
			(0x4001, b"\x81\x80\x01"),
			(0x1fffff, b"\xff\xff\x7f"),
			(0x200000, b"\x80\x80\x80\x01"),
			(0x200001, b"\x81\x80\x80\x01"),
			(0xfffffff, b"\xff\xff\xff\x7f"),
			(0x10000000, b"\x80\x80\x80\x80\x01"),
			(u64::MAX / 2, b"\xff\xff\xff\xff\xff\xff\xff\xff\x7f"),
			(u64::MAX, b"\xff\xff\xff\xff\xff\xff\xff\xff\xff"),
		];

		fn case(n: u64, expected: &[u8]) -> Result<(), AppError> {
			let mut buffer = io::Cursor::new([0; super::MAX_BUFFER_LENGTH as usize]);
			let bytes_written = super::encode(&mut buffer, n).context("Unable to encode")?;
			let buffer = buffer.into_inner();
			ensure!(
				expected == &buffer[..bytes_written as usize],
				"Expected encoding to yield {}, found {}",
				expected.display_hex(),
				buffer[..bytes_written as usize].display_hex(),
			);

			let decoded = super::decode(&mut io::Cursor::new(&buffer)).context("Unable to decode")?;
			ensure!(
				n == decoded,
				"Expected decoding to yield {n:#018x}, found {decoded:#018x}"
			);

			Ok(())
		}

		for (n, expected) in cases {
			if let Err(err) = case(n, expected) {
				panic!("Case {n} failed: {err:?}");
			}
		}
	}
}
