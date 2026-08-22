//! Archive

// TODO: Within a single directory, compress all file names?

pub mod export;
pub mod import;

/// Entry kind
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[derive(strum::FromRepr, strum::EnumIs)]
#[repr(u8)]
pub enum EntryKind {
	Dir     = 0,
	File    = 1,
	Symlink = 2,
}

/// Magic
const MAGIC: [u8; 4] = *b"zduA";

/// Latest archive version
const ARCHIVE_VERSION: u32 = 0;

/// Directory header size
const DIR_HEADER_SIZE: u16 = 16;
