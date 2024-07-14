pub mod header;
pub mod manifest;

use header::Header;
use manifest::ManifestData;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reqwest::header::RANGE;

use std::collections::HashMap;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
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
    pub fn download_files(&self, files: Vec<File>, bundle_cdn: &str) {
        let mut bundles: HashMap<i64, HashMap<String, (u32, u32)>> = HashMap::new();

        for file in files.clone() {
            let mut bundles_min_max: HashMap<String, (u32, u32)> = HashMap::new();
            for (bundle_id, offset, _uncompressed_size, compressed_size) in file.chunks {
                let from = offset;
                let to = offset + compressed_size - 1;

                let bundle_file_name = format!("{:016X}.bundle", bundle_id);

                // Update min and max
                bundles_min_max
                    .entry(bundle_file_name)
                    .and_modify(|(min, max)| {
                        *min = cmp::min(*min, from);
                        *max = cmp::max(*max, to);
                    })
                    .or_insert((from, to));
            }
            bundles.insert(file.id, bundles_min_max);
        }

        let available_parallelism = rayon::current_num_threads();
        println!(
            "Using {} threads to download {} files",
            available_parallelism,
            bundles.len()
        );
        for lol_file in files {
            println!("Downloading {}...", lol_file.name);

            // Download all needed bundles for file into memory
            let bytes_map = Arc::new(Mutex::new(HashMap::new()));
            let bundles_for_file = bundles.get(&lol_file.id).unwrap();
            bundles_for_file
                .par_iter()
                .for_each(|(bundle_file_name, (from, to))| {
                    let client = reqwest::blocking::Client::new();
                    let bundle_url = format!("{}/{}", bundle_cdn, bundle_file_name);

                    let response = client
                        .get(bundle_url.clone())
                        .header(RANGE, format!("bytes={from}-{to}"))
                        .send()
                        .unwrap();
                    let bytes = response.bytes().unwrap();

                    let mut bytes_map = bytes_map.lock().unwrap();
                    bytes_map.insert(bundle_file_name.clone(), bytes.to_vec());
                });

            // Extract the file from the bundles
            println!("Decompressing {}...", lol_file.name);
            if let Some(parent_dir) = Path::new(lol_file.path.as_str()).parent() {
                fs::create_dir_all(parent_dir).unwrap();
            }

            let bytes_map: HashMap<String, Vec<u8>> =
                Arc::try_unwrap(bytes_map).unwrap().into_inner().unwrap();

            let mut file = fs::File::create(lol_file.path.as_str()).unwrap();
            for (bundle_id, offset, uncompressed_size, compressed_size) in lol_file.chunks {
                let bundle_file_name = format!("{:016X}.bundle", bundle_id);

                let (min, _max) = bundles_for_file.get(bundle_file_name.as_str()).unwrap();
                let from = offset - min;
                let to = offset + compressed_size - 1 - min;

                let bundle_bytes = bytes_map.get(bundle_file_name.as_str()).unwrap();
                let uncompressed_size: usize = (uncompressed_size).try_into().unwrap();
                let bundle_slice = &bundle_bytes[from as usize..to as usize + 1];

                let decompressed_chunk =
                    zstd::bulk::decompress(bundle_slice, uncompressed_size).unwrap();

                file.write_all(&decompressed_chunk).unwrap();
            }
        }
    }
}
