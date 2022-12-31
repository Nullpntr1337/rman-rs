use crate::error::Error;
use crate::structs::Cursor;

#[derive(Clone, Copy, Debug, Default)]
pub struct FileHeader {
    pub magic: u32,
    pub major: u8,
    pub minor: u8,
    pub flags: u16,
    pub offset: u32,
    pub compressed_size: u32,
    pub manifest_id: u64,
    pub uncompressed_size: u32,
}

impl TryFrom<&[u8]> for FileHeader {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut cursor = Cursor::from(bytes);
        let magic = cursor.read_u32()?;

        // N A M R (RMAN bacwards because I am reading this as an u32, instead
        // of as an array of chars)
        if magic != 0x4E414D52 {
            return Err(Error::InvalidMagicBytes(magic));
        }

        let major = cursor.read_u8()?;
        let minor = cursor.read_u8()?;

        if major != 2 {
            #[cfg(not(feature = "version_error"))]
            {
                println!("Warning: Invalid major version. Parsing the manfiset may not work.");
                println!("If you want the crate to throw an error instead, you can enable the \"version_error\" feature");
            }
            #[cfg(feature = "version_error")]
            return Err(Error::InvalidMajor(major));
        }
        if major == 2 && minor != 0 {
            #[cfg(not(feature = "version_error"))]
            {
                println!(
                    "Info: Invalid minor version. Parsing the manfiset will probably still work."
                );
                println!("If you want the crate to throw an error instead, you can enable the \"version_error\" feature");
            }
            #[cfg(feature = "version_error")]
            return Err(Error::InvalidMinor(minor));
        }

        let flags = cursor.read_u16()?;

        let offset = cursor.read_u32()?;
        let compressed_size = cursor.read_u32()?;
        let manifest_id = cursor.read_u64()?;
        let uncompressed_size = cursor.read_u32()?;

        let file_header = Self {
            magic,
            major,
            minor,
            flags,
            offset,
            compressed_size,
            manifest_id,
            uncompressed_size,
        };

        Ok(file_header)
    }
}