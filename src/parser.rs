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

    /// Downloads and processes a list of files.
    ///
    /// # Arguments
    ///
    /// * `files` - A vector of `File` structs representing the files to download.
    /// * `bundle_cdn` - A string slice representing the URL of the bundle CDN.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    ///
    /// * The required bundle for a file is not found in the `bundles` map.
    /// * It fails to unwrap the `Arc` containing the bytes map.
    /// * It fails to obtain a lock on the `Mutex` containing the bytes map.
    /// * It fails to create the necessary parent directories for a file.
    /// * It fails to create a file for writing the decompressed data.
    /// * It fails to convert the size of the uncompressed data chunk.
    /// * It fails to decompress the data chunk.
    /// * It fails to write the decompressed data chunk to a file.
    pub fn download_files(&self, files: Vec<File>, bundle_cdn: &str) {
        let bundles = Self::prepare_bundles(&files);

        let available_parallelism = rayon::current_num_threads();
        println!(
            "Using {} threads to download {} files",
            available_parallelism,
            bundles.len()
        );

        for file in files {
            println!("Downloading {}...", file.name);

            let bytes_map = Arc::new(Mutex::new(HashMap::new()));
            let bundles_for_file = bundles
                .get(&file.id)
                .expect("Error: Bundle was not found for the given file ID");
            let client = reqwest::blocking::Client::new();

            Self::download_bundles(&client, &bytes_map, bundles_for_file, bundle_cdn);

            println!("Decompressing {}...", file.name);
            Self::create_parent_dir(&file.path);

            let bytes_map = Arc::try_unwrap(bytes_map)
                .expect("Error: Failed to unwrap Arc containing bytes map")
                .into_inner()
                .expect("Error: Failed to obtain Mutex guard for bytes map");

            Self::decompress_and_write_file(&file, &bytes_map, bundles_for_file);
        }
    }

    fn prepare_bundles(files: &[File]) -> HashMap<i64, HashMap<String, (u32, u32)>> {
        let mut bundles: HashMap<i64, HashMap<String, (u32, u32)>> = HashMap::new();

        for file in files {
            let mut bundles_min_max: HashMap<String, (u32, u32)> = HashMap::new();
            for (bundle_id, offset, _uncompressed_size, compressed_size) in file.chunks.clone() {
                let from = offset;
                let to = offset + compressed_size - 1;
                let bundle_file_name = format!("{bundle_id:016X}.bundle");

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

        bundles
    }

    fn download_bundles(
        client: &reqwest::blocking::Client,
        bytes_map: &Arc<Mutex<HashMap<String, Vec<u8>>>>,
        bundles_for_file: &HashMap<String, (u32, u32)>,
        bundle_cdn: &str,
    ) {
        bundles_for_file
            .par_iter()
            .for_each(|(bundle_file_name, (from, to))| {
                let bundle_url = format!("{bundle_cdn}/{bundle_file_name}");
                let expected_length = (to - from + 1) as usize;

                for attempt in 1..=3 {
                    let result = client
                        .get(bundle_url.clone())
                        .header(RANGE, format!("bytes={from}-{to}"))
                        .send()
                        .and_then(reqwest::blocking::Response::bytes);

                    match result {
                        Ok(bytes) => {
                            if bytes.len() == expected_length {
                                bytes_map
                                    .lock()
                                    .expect("Error: Failed to lock bytes_map for insertion")
                                    .insert(bundle_file_name.clone(), bytes.to_vec());
                                break;
                            }
                            eprintln!(
                                "Error: Byte length mismatch for {}: expected {}, got {}",
                                bundle_file_name,
                                expected_length,
                                bytes.len()
                            );
                        }
                        Err(e) => {
                            eprintln!("Error: Attempt {attempt}/3 to fetch {bundle_file_name} failed: {e}");
                            if attempt == 3 {
                                eprintln!("Error: Failed to fetch {bundle_file_name} after 3 attempts");
                            }
                        }
                    }
                }
            });
    }

    fn create_parent_dir(file_path: &str) {
        if let Some(parent_dir) = Path::new(file_path).parent() {
            fs::create_dir_all(parent_dir)
                .expect("Error: Failed to create parent directories for the file path");
        }
    }

    fn decompress_and_write_file(
        lol_file: &File,
        bytes_map: &HashMap<String, Vec<u8>>,
        bundles_for_file: &HashMap<String, (u32, u32)>,
    ) {
        let mut file = fs::File::create(&lol_file.path)
            .expect("Error: Failed to create file for decompressed data");

        for (bundle_id, offset, uncompressed_size, compressed_size) in &lol_file.chunks {
            let bundle_file_name = format!("{bundle_id:016X}.bundle");
            let (min, _max) = bundles_for_file
                .get(&bundle_file_name)
                .expect("Error: Bundle min value not found in the bundles map");
            let from = offset - min;
            let to = offset + compressed_size - 1 - min;

            let bundle_bytes = bytes_map
                .get(&bundle_file_name)
                .expect("Error: Bundle bytes not found in the bytes map");
            let uncompressed_size: usize = (*uncompressed_size)
                .try_into()
                .expect("Error: Failed to convert uncompressed size to usize");
            let bundle_slice = &bundle_bytes[(from as usize)..=(to as usize)];

            let decompressed_chunk = zstd::bulk::decompress(bundle_slice, uncompressed_size)
                .expect("Error: Failed to decompress the data chunk");

            file.write_all(&decompressed_chunk)
                .expect("Error: Failed to write decompressed chunk to file");
        }
    }
}
