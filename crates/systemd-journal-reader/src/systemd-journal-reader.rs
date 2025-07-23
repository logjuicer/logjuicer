//! A library for reading systemd journal files in a streaming fashion.
//!
//! This crate provides a `JournalReader` that can be used to parse
//! journal files and iterate over their entries. It is designed to be
//! memory-efficient and safe, processing one entry at a time without
//! loading the entire file into memory or using unsafe code. This version
//! is designed for Read-only streams (like network sockets) and does not
//! require the Seek trait.

use std::collections::HashMap;
use std::convert::TryInto;
use std::io::{self, Read};
use std::rc::Rc;

// Constants from the systemd journal file format specification.
const SIGNATURE: &[u8; 8] = b"LPKSHHRH";
const HEADER_SIZE_MIN: u64 = 240;

const OBJECT_DATA: u8 = 1;
const OBJECT_ENTRY: u8 = 3;

const HEADER_INCOMPATIBLE_COMPRESSED_ZSTD: u32 = 1 << 3;
const HEADER_INCOMPATIBLE_COMPACT: u32 = 1 << 4;
const OBJECT_COMPRESSED_ZSTD: u8 = 1 << 2;

/// A helper function to read a u64 from a reader.
fn read_u64<R: Read>(reader: &mut R) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Represents the header of a systemd journal file.
#[derive(Debug, Clone)]
struct Header {
    signature: [u8; 8],
    incompatible_flags: u32,
    header_size: u64,
    arena_size: u64,
}

fn slice2io(e: std::array::TryFromSliceError) -> io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}

impl Header {
    /// Parses the journal header from a reader.
    fn read_from<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut buf = [0u8; 240]; // Read the minimum header size
        reader.read_exact(&mut buf)?;

        let signature: [u8; 8] = buf[0..8].try_into().map_err(slice2io)?;
        let incompatible_flags = u32::from_le_bytes(buf[12..16].try_into().map_err(slice2io)?);
        let header_size = u64::from_le_bytes(buf[88..96].try_into().map_err(slice2io)?);
        let arena_size = u64::from_le_bytes(buf[96..104].try_into().map_err(slice2io)?);

        Ok(Header {
            signature,
            incompatible_flags,
            header_size,
            arena_size,
        })
    }
}

/// Represents the header of an object within the journal file.
#[derive(Debug, Clone)]
struct ObjectHeader {
    object_type: u8,
    flags: u8,
    size: u64,
}

impl ObjectHeader {
    /// Parses an object header from a reader.
    fn read_from<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut buf = [0u8; 16];
        reader.read_exact(&mut buf)?;
        Ok(ObjectHeader {
            object_type: buf[0],
            flags: buf[1],
            size: u64::from_le_bytes(buf[8..16].try_into().map_err(slice2io)?),
        })
    }
}

/// Represents an entry object, which ties together multiple data objects.
#[derive(Debug, Clone)]
struct EntryObject {
    realtime: u64,
}

impl EntryObject {
    /// Parses an entry object's fixed fields from a reader.
    fn read_from<R: Read>(reader: &mut R) -> io::Result<Self> {
        // We only care about the realtime timestamp for this implementation.
        // seqnum: u64
        let _ = read_u64(reader)?;
        // realtime: u64
        let realtime = read_u64(reader)?;
        // monotonic: u64
        let _ = read_u64(reader)?;
        // boot_id: [u8; 16]
        let mut boot_id_buf = [0u8; 16];
        reader.read_exact(&mut boot_id_buf)?;
        // xor_hash: u64
        let _ = read_u64(reader)?;

        Ok(EntryObject { realtime })
    }
}

/// A journal entry.
#[derive(Debug)]
pub struct Entry {
    /// The __REALTIME_TIMESTAMP value.
    pub realtime: u64,
    /// The entry fields.
    pub fields: HashMap<Rc<str>, Rc<str>>,
}

/// Reads systemd journal files in a streaming manner from a Read-only source.
pub struct JournalReader<R: Read> {
    reader: std::io::BufReader<R>,
    header: Header,
    current_offset: u64,
    data_object_cache: HashMap<u64, (Rc<str>, Rc<str>)>,
}

impl<R: Read> JournalReader<R> {
    /// Creates a new `JournalReader` from a readable source.
    pub fn new(mut file: R) -> io::Result<JournalReader<R>> {
        let header = Header::read_from(&mut file)?;

        if &header.signature != SIGNATURE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid journal file signature",
            ));
        }

        if header.header_size < HEADER_SIZE_MIN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Header size is too small",
            ));
        }

        // Discard the rest of the header if it's larger than the minimum
        if header.header_size > HEADER_SIZE_MIN {
            let to_discard = header.header_size - HEADER_SIZE_MIN;
            io::copy(&mut (&mut file).take(to_discard), &mut io::sink())?;
        }

        let current_offset = header.header_size;

        Ok(JournalReader {
            reader: std::io::BufReader::new(file),
            header,
            current_offset,
            data_object_cache: HashMap::new(),
        })
    }

    /// Reads the next log entry from the journal stream.
    /// Note: This method buffers data objects in memory. For very large journal
    /// files without frequent entries, memory usage can grow.
    pub fn next_entry(&mut self) -> Option<Entry> {
        while self.current_offset < self.header.header_size + self.header.arena_size {
            let object_start_offset = self.current_offset;

            let object_header = match ObjectHeader::read_from(&mut self.reader) {
                Ok(h) => h,
                Err(_) => return None, // End of file or read error
            };

            let object_header_size = 16u64;
            let payload_size = object_header.size.saturating_sub(object_header_size);

            let entry = match object_header.object_type {
                OBJECT_ENTRY => {
                    let entry_map = self.parse_entry_object_payload(payload_size).ok()?;
                    Some(entry_map)
                }
                OBJECT_DATA => {
                    if let Some(data_map) =
                        self.parse_data_object_payload(object_header.flags, payload_size)
                    {
                        self.data_object_cache.insert(object_start_offset, data_map);
                    };
                    None
                }
                _ => {
                    // Skip other object types by discarding their payload
                    io::copy(&mut (&mut self.reader).take(payload_size), &mut io::sink()).ok()?;
                    None
                }
            };

            let padded_size = (object_header.size + 7) & !7;
            let padding = padded_size - object_header.size;
            if padding > 0 {
                io::copy(&mut (&mut self.reader).take(padding), &mut io::sink()).ok()?;
            }
            self.current_offset = object_start_offset + padded_size;
            if entry.is_some() {
                return entry;
            }
        }
        None
    }

    /// Parses the payload of a data object, returning the contained fields.
    fn parse_data_object_payload(
        &mut self,
        flags: u8,
        payload_size: u64,
    ) -> Option<(Rc<str>, Rc<str>)> {
        let is_compact = (self.header.incompatible_flags & HEADER_INCOMPATIBLE_COMPACT) != 0;

        // The fixed fields of DataObject are part of the payload now.
        // We must read and discard them to get to the actual data.
        // hash, next_hash_offset, next_field_offset, entry_offset, entry_array_offset, n_entries
        let mut data_object_fixed_size = 8 * 6;
        if is_compact {
            data_object_fixed_size += 4 + 4; // tail_entry_array_offset + tail_entry_array_n_entries
        }

        if payload_size < data_object_fixed_size {
            io::copy(&mut (&mut self.reader).take(payload_size), &mut io::sink()).ok()?;
            return None;
        }
        io::copy(
            &mut (&mut self.reader).take(data_object_fixed_size),
            &mut io::sink(),
        )
        .ok()?;

        let data_payload_size = payload_size - data_object_fixed_size;
        let mut payload_buf = vec![0u8; data_payload_size as usize];
        self.reader.read_exact(&mut payload_buf).ok()?;

        let final_payload =
            if (self.header.incompatible_flags & HEADER_INCOMPATIBLE_COMPRESSED_ZSTD != 0)
                && (flags & OBJECT_COMPRESSED_ZSTD != 0)
            {
                zstd::decode_all(payload_buf.as_slice()).unwrap_or_default()
            } else {
                payload_buf
            };

        let data_str = String::from_utf8_lossy(&final_payload);
        let mut parts = data_str.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next().unwrap_or("");

        Some((key.into(), value.into()))
    }

    /// Parses the payload of an entry object, constructing the entry map from the cache.
    fn parse_entry_object_payload(&mut self, payload_size: u64) -> io::Result<Entry> {
        let entry_object = EntryObject::read_from(&mut self.reader)?;

        let mut fields = HashMap::new();

        let entry_object_fixed_size = 8 + 8 + 8 + 16 + 8;
        let mut items_payload_size = payload_size.saturating_sub(entry_object_fixed_size);

        let is_compact = (self.header.incompatible_flags & HEADER_INCOMPATIBLE_COMPACT) != 0;
        let item_size = if is_compact { 4 } else { 16 };

        while items_payload_size >= item_size {
            let data_object_offset = if is_compact {
                let mut buf = [0u8; 4];
                self.reader.read_exact(&mut buf)?;
                u32::from_le_bytes(buf) as u64
            } else {
                read_u64(&mut self.reader)?
            };

            if !is_compact {
                // Skip the hash
                let _ = read_u64(&mut self.reader)?;
            }

            if let Some((k, v)) = self.data_object_cache.get(&data_object_offset) {
                fields.insert(k.clone(), v.clone());
            }
            items_payload_size -= item_size;
        }

        // Skip any remaining padding in the entry object payload
        if items_payload_size > 0 {
            io::copy(
                &mut (&mut self.reader).take(items_payload_size),
                &mut io::sink(),
            )?;
        }

        Ok(Entry {
            realtime: entry_object.realtime,
            fields,
        })
    }
}
