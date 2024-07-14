pub mod header;
pub mod manifest;

use header::Header;
use manifest::ManifestData;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reqwest::header::RANGE;

use std::collections::HashMap;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::thread::{self, available_parallelism};
use std::{cmp, fs};

use log::debug;

use crate::{File, ManifestError, Result};

/// Main parser object.
///
/// Depending on the function you call, it either parses a manifest
/// [from reader][crate::RiotManifest::from_reader] or [a file][crate::RiotManifest::from_path].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RiotManifest {
    /// Parsed file header data.
    ///
    /// Stores information like [magic bytes](crate::Header::magic),
    /// [version](crate::Header::major), [flags](crate::Header::flags),
    /// [size](crate::Header::compressed_size), [offset](crate::Header::offset), etc.
    pub header: Header,
    /// Parsed flatbuffer data.
    ///
    /// Stores all of the [flatbuffer entries][crate::entries], as well as the [parsed files][crate::File].
    pub data: ManifestData,
}

impl RiotManifest {
    /// Loads data from a file and parses it.
    ///
    /// This is just a convenience method that [opens a file][std::fs::File::open],
    /// [buffers it][std::io::BufReader] and calls [`RiotManifest::from_reader`].
    ///
    /// # Errors
    ///
    /// If reading a file fails, the error [`IoError`][crate::ManifestError::IoError] is
    /// returned.
    ///
    /// If parsing fails, it propagates an error from [`RiotManifest::from_reader`].
    ///
    /// # Examples
    ///
    /// See
    /// [parsing a manifest file from path](index.html#example-parsing-a-manifest-file-from-path).
    ///
    /// [`RiotManifest::from_reader`]: crate::RiotManifest::from_reader
    pub fn from_path<P: AsRef<Path>>(
        path: P,
        flatbuffer_verifier_options: Option<&flatbuffers::VerifierOptions>,
    ) -> Result<Self> {
        let file = fs::File::open(path)?;
        let mut reader = BufReader::new(file);
        Self::from_reader(&mut reader, flatbuffer_verifier_options)
    }

    /// Main parser method.
    ///
    /// Brief overview on how parsing the manifest is done:
    /// - attempts to [parse the header][crate::Header::from_reader]
    /// - [seeks][std::io::Seek] to the [offset](crate::Header::offset)
    /// - reads [x amount](crate::Header::compressed_size) of bytes to buffer
    /// - [decompresses][zstd::bulk::decompress] read bytes
    /// - decompressed data is a [flatbuffer binary], that is then
    /// [parsed][crate::ManifestData::parse].
    ///
    /// # Errors
    ///
    /// If parsing the header fails, it propagates an error from
    /// [`Header::from_reader`][crate::Header::from_reader].
    ///
    /// If seeking to offset fails, the error [`SeekError`][crate::ManifestError::SeekError] is
    /// returned.
    ///
    /// If converting [`compressed_size`](crate::Header::compressed_size) or
    /// [`uncompressed_size`](crate::Header::uncompressed_size) to [`usize`] fails, the error
    /// [`ConversionFailure`][crate::ManifestError::ConversionFailure] is returned.
    ///
    /// If reading compressed flatbuffer data fails, the error
    /// [`IoError`][crate::ManifestError::IoError] is returned.
    ///
    /// If zstd decompression fails, the error
    /// [`ZstdDecompressError`][crate::ManifestError::ZstdDecompressError] is returned.
    ///
    /// If parsing flatbuffer binary fails, it propagates an error from
    /// [`ManifestData::parse`][crate::ManifestData::parse].
    ///
    /// [flatbuffer binary]: https://github.com/ev3nvy/rman-schema
    pub fn from_reader<R: Read + Seek>(
        mut reader: R,
        flatbuffer_verifier_options: Option<&flatbuffers::VerifierOptions>,
    ) -> Result<Self> {
        let header = Header::from_reader(&mut reader)?;

        if let Err(error) = reader.seek(SeekFrom::Start(header.offset.into())) {
            return Err(ManifestError::SeekError(error));
        };

        debug!("Attempting to convert \"compressed_size\" into \"usize\".");
        let compressed_size: usize = header.compressed_size.try_into()?;
        debug!("Successfully converted \"compressed_size\" into \"usize\".");

        let mut buf = vec![0u8; compressed_size];
        reader.read_exact(&mut buf)?;

        debug!("Attempting to convert \"uncompressed_size\" into \"usize\".");
        let uncompressed_size: usize = header.uncompressed_size.try_into()?;
        debug!("Successfully converted \"uncompressed_size\" into \"usize\".");

        let decompressed = match zstd::bulk::decompress(&buf, uncompressed_size) {
            Ok(result) => result,
            Err(error) => return Err(ManifestError::ZstdDecompressError(error)),
        };

        let data = ManifestData::parse(&decompressed, flatbuffer_verifier_options)?;

        Ok(Self { header, data })
    }

    /// Example
    pub fn download_files(&self, files: Vec<File>, bundle_cdn: &str, output_path: &str) {
        let mut bundle_urls: HashMap<String, (u32, u32)> = HashMap::new();

        for file in files {
            for (bundle_id, offset, _uncompressed_size, compressed_size) in file.chunks {
                let from = offset;
                let to = offset + compressed_size - 1;

                let bundle_url = format!("{}/{:016X}.bundle", bundle_cdn, bundle_id);

                // Update min and max
                bundle_urls
                    .entry(bundle_url)
                    .and_modify(|(min, max)| {
                        *min = cmp::min(*min, from);
                        *max = cmp::max(*max, to);
                    })
                    .or_insert((from, to));
            }
        }

        // Process the HashMap in parallel
        let available_parallelism = rayon::current_num_threads();
        println!("Using {} threads", available_parallelism);

        bundle_urls.par_iter().for_each(|(bundle_url, (from, to))| {
            let client = reqwest::blocking::Client::new();
            let response = client
                .get(bundle_url)
                .header(RANGE, format!("bytes={from}-{to}"))
                .send()
                .unwrap();

            let _bytes = response.bytes().unwrap();
        });

        //Start threads(cpu core count) to download all the bundles with range of min -> max

        //Run code here to parse the bundles into actual files
    }

    // pub fn download_files(files: Vec<File>, bundle_cdn: String, output: String) -> Result<()> {
    //     let client = Client::new();
    //     let mut bundle_byte_map: HashMap<&i64, Vec<u8>> = HashMap::new();

    //     for (bundle_id, offset, uncompressed_size, compressed_size) in &self.chunks {
    //         let from = *offset;
    //         let to = from + compressed_size - 1;

    //         if !bundle_byte_map.contains_key(bundle_id) {
    //             let response = client
    //                 .get(format!("{}/{:016X}.bundle", bundle_url.as_str(), bundle_id))
    //                 .send()
    //                 .await?;

    //             let bytes = response.bytes().await?.to_vec(); // Store the bytes as a Vec<u8>
    //             bundle_byte_map.insert(bundle_id, bytes);
    //         }

    //         debug!("Attempting to convert \"uncompressed_size\" into \"usize\".");
    //         let uncompressed_size: usize = (*uncompressed_size).try_into()?;
    //         debug!("Successfully converted \"uncompressed_size\" into \"usize\".");

    //         // Get the bundle from the hashmap
    //         let bundle_data = bundle_byte_map.get(bundle_id).unwrap();
    //         let bundle_bytes = &bundle_data[from as usize..to as usize + 1];

    //         let decompressed_chunk = match zstd::bulk::decompress(bundle_bytes, uncompressed_size) {
    //             Ok(result) => result,
    //             Err(error) => return Err(ManifestError::ZstdDecompressError(error)),
    //         };

    //         // Write the relevant slice to the writer
    //         writer.write_all(&decompressed_chunk)?;
    //     }

    //     bundle_byte_map.clear();
    //     Ok(())
    // }
}
