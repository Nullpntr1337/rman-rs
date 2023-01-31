use crate::generated::rman::File;

/// Single file entry object.
///
/// This is identical to the schema in [rman-schema][rman-schema] and exists to provide a
/// persistent structure for the FileEntry.
///
/// [rman-schema]: https://github.com/ev3nvy/rman-schema
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Id of the file entry.
    pub id: u64,
    /// Id of the directory entry, to which it belongs.
    pub directory_id: u64,
    /// Size of the file entry in bytes.
    pub size: u32,
    /// Name of the file entry.
    pub name: String,
    /// Applicable languages, stored as a bit mask.
    pub language_mask: u64,
    /// Field with an unknown function and type (it might also be an [`i8`]).
    pub unk5: u8,
    /// Field with an unknown function and type (it might also be an [`i8`]).
    pub unk6: u8,
    /// A vector of [chunk ids](crate::entries::ChunkEntry::id) that make up the file.
    pub chunk_ids: Vec<u64>,
    /// Field with an unknown function and type (it might also be an [`i8`]).
    ///
    /// NOTE: seems to always be 1 when a part of `.app` file on macOS.
    pub unk8: u8,
    /// Symbolic link of the file entry.
    pub symlink: String,
    /// Field with an unknown function and type (it might also be an [`i16`]).
    pub unk10: u16,
    /// Id of the param entry, which provides info about content-defined chunking.
    pub param_id: u8,
    /// Permissions for the given file entry.
    pub permissions: u8,
}

impl From<File<'_>> for FileEntry {
    fn from(file: File) -> Self {
        let id = file.id();
        let directory_id = file.directory_id();
        let size = file.size_();
        let name = file.name().unwrap_or_default().to_owned();
        let language_mask = file.language_mask();
        let unk5 = file.unk5();
        let unk6 = file.unk6();
        let chunk_ids = file.chunk_ids().unwrap_or_default();
        let unk8 = file.unk8();
        let symlink = file.symlink().unwrap_or_default().to_owned();
        let unk10 = file.unk10();
        let param_id = file.param_id();
        let permissions = file.permissions();

        let chunk_ids = chunk_ids.iter().collect();

        Self {
            id,
            directory_id,
            size,
            name,
            language_mask,
            unk5,
            unk6,
            chunk_ids,
            unk8,
            symlink,
            unk10,
            param_id,
            permissions,
        }
    }
}
