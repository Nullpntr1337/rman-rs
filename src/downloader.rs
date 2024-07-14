//! Collection of entries from the parsed flatbuffer schema.
//!
//! NOTE: these structs are only useful for viewing the internal structure of the parsed
//! flatbuffer schema. If you just wish to see the containing files and download them, see
//! [File][crate::File].

///example
pub fn download_files() {
    println!("hello")
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
