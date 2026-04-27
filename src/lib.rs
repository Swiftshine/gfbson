use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap, ser::SerializeSeq};
use std::{
    collections::HashMap,
    io::{Cursor, Read, Seek, Write},
};

const BSON_VERSION: u32 = 3;
const PLACEHOLDER_VALUE: u32 = 0xDEADCAFE;

/* Utility */

#[derive(thiserror::Error, Debug)]
pub enum GFBSONError {
    #[error("input/output error")]
    Io(#[from] std::io::Error),
    #[error("invalid magic.")]
    InvalidMagicError,
    #[error("invalid/unsupported node type. found [{0}]")]
    InvalidNodeType(u32),
    #[error("unexpected node type. expected [{expected}], found [{found}]")]
    UnexpectedNodeType { expected: u32, found: u32 },
    #[error("invalid string index. found [{found}], but there are only [{count}] elements")]
    InvalidStringIndex { found: usize, count: usize },
    #[error("cannot use automatic endianness when writing.")]
    NoAutoEndian,
}

pub type GFBSONResult<T> = Result<T, GFBSONError>;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    #[default]
    Auto,
    Big,
    Little,
}

/* Reading */

struct Reader<'a> {
    cursor: Cursor<&'a [u8]>,
    strings: Vec<String>,
    endian: Endianness,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], endian: Endianness) -> Self {
        Self {
            cursor: Cursor::new(data),
            strings: Vec::new(),
            endian,
        }
    }

    fn read_u32(&mut self) -> GFBSONResult<u32> {
        match self.endian {
            Endianness::Big => Ok(self.cursor.read_u32::<BigEndian>()?),
            Endianness::Little => Ok(self.cursor.read_u32::<LittleEndian>()?),
            _ => unreachable!(),
        }
    }

    fn read_i32(&mut self) -> GFBSONResult<i32> {
        match self.endian {
            Endianness::Big => Ok(self.cursor.read_i32::<BigEndian>()?),
            Endianness::Little => Ok(self.cursor.read_i32::<LittleEndian>()?),
            _ => unreachable!(),
        }
    }

    fn read_f32(&mut self) -> GFBSONResult<f32> {
        match self.endian {
            Endianness::Big => Ok(self.cursor.read_f32::<BigEndian>()?),
            Endianness::Little => Ok(self.cursor.read_f32::<LittleEndian>()?),
            _ => unreachable!(),
        }
    }

    fn set_position(&mut self, position: u64) {
        self.cursor.set_position(position);
    }

    fn position(&self) -> u64 {
        self.cursor.position()
    }

    fn skip(&mut self, skip_amount: u64) {
        let _ = self.cursor.seek_relative(skip_amount as i64);
    }

    fn read_string_entry(&mut self) -> GFBSONResult<StringEntry> {
        let offset = self.read_u32()?;
        let length = self.read_u32()?;

        Ok(StringEntry { offset, length })
    }

    fn read_referenced_string(&mut self) -> GFBSONResult<String> {
        let index = self.read_u32()? as usize;
        self.strings
            .get(index)
            .cloned()
            .ok_or(GFBSONError::InvalidStringIndex {
                found: index,
                count: self.strings.len(),
            })
    }

    fn read_root(mut self) -> GFBSONResult<Root> {
        let magic = {
            let mut m = [0u8; 4];
            self.cursor.read_exact(&mut m)?;
            m
        };

        if &magic != b"BSON" {
            return Err(GFBSONError::InvalidMagicError);
        }

        if matches!(self.endian, Endianness::Auto) {
            let raw_version = self.cursor.read_u32::<BigEndian>()?;

            self.endian = if raw_version == BSON_VERSION {
                Endianness::Big
            } else {
                Endianness::Little
            };
        } else {
            self.skip(4);
        }

        let node_offset = self.read_u32()?;
        self.skip(4); // size of entire BSON file
        self.strings = self.read_strings(node_offset)?;

        // read root node
        self.validate_node_type(RawNodeType::Root)?;
        self.skip(4); // root size

        let root = Root {
            object_node: self.read_node()?,
        };

        Ok(root)
    }

    fn read_strings(&mut self, node_offset: u32) -> GFBSONResult<Vec<String>> {
        let strings = {
            // skip ahead to string table
            self.skip(4); // root type
            let root_size = self.read_u32()? as u64;
            self.skip(root_size);

            self.validate_node_type(RawNodeType::StringTable)?;

            let table_size = self.read_u32()?;

            // each entry within the string lookup table consists of 8 bytes:
            //  - a u32 for the offset within the string bank
            //  - a u32 for the length of that string

            // divide by the size of 2 u32s
            let string_count = table_size / 8;

            let string_entries = (0..string_count)
                .map(|_| self.read_string_entry())
                .collect::<Result<Vec<_>, _>>()?;

            // the cursor should now be at the end of the string bank node

            self.validate_node_type(RawNodeType::StringBank)?;

            let bank_size = self.read_u32()? as usize;
            let bank_start = self.position() as usize;
            let bank_end = bank_size + bank_start;
            let bank_slice = &self.cursor.get_ref()[bank_start..bank_end];

            let strings: Vec<String> = string_entries
                .iter()
                .map(|entry| {
                    let string_start = entry.offset as usize;
                    // subtract 1 from the length because it includes the null-terminator
                    let string_end = string_start + (entry.length - 1) as usize;
                    let string_slice = &bank_slice[string_start..string_end];

                    String::from_utf8_lossy(string_slice).to_string()
                })
                .collect();

            // skip back to root node
            self.set_position(node_offset as u64);

            strings
        };

        Ok(strings)
    }

    /// Reads a node type and size and reads data based on that information.
    fn read_node(&mut self) -> GFBSONResult<Node> {
        let node_type = self.read_u32()?;
        self.skip(4); // node size

        let node_type = RawNodeType::try_from(node_type)
            .map_err(|_| GFBSONError::InvalidNodeType(node_type))?;

        match node_type {
            RawNodeType::Object => Ok(Node::Object {
                key: self.read_referenced_string()?,
                nodes: {
                    let num_nodes = self.read_u32()?;

                    (0..num_nodes)
                        .map(|_| self.read_node())
                        .collect::<Result<Vec<_>, _>>()?
                },
            }),

            RawNodeType::Array => Ok(Node::Array {
                key: self.read_referenced_string()?,
                nodes: {
                    let num_nodes = self.read_u32()?;

                    (0..num_nodes)
                        .map(|_| self.read_node())
                        .collect::<Result<Vec<_>, _>>()?
                },
            }),

            RawNodeType::Integer => Ok(Node::Integer {
                key: self.read_referenced_string()?,
                value: self.read_i32()?,
            }),

            RawNodeType::Float => Ok(Node::Float {
                key: self.read_referenced_string()?,
                value: self.read_f32()?,
            }),

            RawNodeType::String => Ok(Node::String {
                key: self.read_referenced_string()?,
                value: self.read_referenced_string()?,
            }),

            RawNodeType::Bool => Ok(Node::Bool {
                key: self.read_referenced_string()?,
                value: self.read_u32()? == 1,
            }),
            _ => todo!(),
        }
    }

    /// Reads a node type, compares it to `expected`, and returns an error if it doesn't match.
    fn validate_node_type(&mut self, expected: RawNodeType) -> GFBSONResult<()> {
        let found = self.read_u32()?;
        if found != expected as u32 {
            return Err(GFBSONError::UnexpectedNodeType {
                expected: expected as u32,
                found,
            });
        }
        Ok(())
    }
}

/// Reads from a BSON file.
pub fn read(data: &[u8], endian: Endianness) -> GFBSONResult<Root> {
    Reader::new(data, endian).read_root()
}

#[cfg(feature = "json")]
pub fn from_json(json_str: &str) -> Result<Root, serde_json::Error> {
    let v: serde_json::Value = serde_json::from_str(json_str)?;
    Ok(Root::from_json_value(v))
}

/* Writing */

#[derive(Debug)]
struct Writer {
    buffer: Vec<u8>,
    // key, index
    string_map: HashMap<String, u32>,
    string_order: Vec<String>,
    endian: Endianness,
}

impl Writer {
    fn new(endian: Endianness) -> Self {
        Self {
            buffer: Vec::new(),
            string_map: HashMap::new(),
            string_order: Vec::new(),
            endian: endian,
        }
    }

    fn write_u32(&mut self, val: u32) -> GFBSONResult<()> {
        match self.endian {
            Endianness::Big => self.buffer.write_u32::<BigEndian>(val)?,
            Endianness::Little => self.buffer.write_u32::<LittleEndian>(val)?,
            _ => unreachable!(),
        }

        Ok(())
    }

    fn collect_strings(&mut self, node: &Node) {
        let mut add_string = |string: &String| {
            if !self.string_map.contains_key(string) {
                self.string_map
                    .insert(string.clone(), self.string_order.len() as u32);
                self.string_order.push(string.clone());
            }
        };

        match node {
            Node::Object { key, nodes } | Node::Array { key, nodes } => {
                add_string(key);
                for n in nodes {
                    self.collect_strings(n);
                }
            }
            Node::Integer { key, .. } | Node::Float { key, .. } | Node::Bool { key, .. } => {
                add_string(key);
            }
            Node::String { key, value } => {
                add_string(key);
                add_string(value);
            }
        }
    }

    fn write_node(&mut self, node: &Node) -> GFBSONResult<()> {
        let start_pos = self.buffer.len();

        let node_type = match node {
            Node::Object { .. } => RawNodeType::Object,
            Node::Array { .. } => RawNodeType::Array,
            Node::Integer { .. } => RawNodeType::Integer,
            Node::Float { .. } => RawNodeType::Float,
            Node::String { .. } => RawNodeType::String,
            Node::Bool { .. } => RawNodeType::Bool,
        };

        self.write_u32(node_type as u32)?;
        self.write_u32(PLACEHOLDER_VALUE)?; // size placeholder

        // write key index
        let key = match node {
            Node::Object { key, .. }
            | Node::Array { key, .. }
            | Node::Integer { key, .. }
            | Node::Float { key, .. }
            | Node::String { key, .. }
            | Node::Bool { key, .. } => key,
        };

        let key_index = *self.string_map.get(key).unwrap();
        self.write_u32(key_index)?;

        // write content
        match node {
            Node::Object { nodes, .. } | Node::Array { nodes, .. } => {
                self.write_u32(nodes.len() as u32)?;
                for n in nodes {
                    self.write_node(n)?;
                }
            }
            Node::Integer { value, .. } => match self.endian {
                Endianness::Big => {
                    self.buffer.write_i32::<BigEndian>(*value)?;
                }

                Endianness::Little => {
                    self.buffer.write_i32::<LittleEndian>(*value)?;
                }

                _ => unreachable!(),
            },
            Node::Float { value, .. } => match self.endian {
                Endianness::Big => {
                    self.buffer.write_f32::<BigEndian>(*value)?;
                }

                Endianness::Little => {
                    self.buffer.write_f32::<LittleEndian>(*value)?;
                }

                _ => unreachable!(),
            },
            Node::String { value, .. } => {
                let val_idx = *self.string_map.get(value).unwrap();
                self.write_u32(val_idx)?;
            }
            Node::Bool { value, .. } => {
                self.write_u32(if *value { 1 } else { 0 })?;
            }
        }

        // fill in size
        let end_pos = self.buffer.len();
        let size = (end_pos - start_pos - 8) as u32;
        let mut size_slice = &mut self.buffer[start_pos + 4..start_pos + 8];

        match self.endian {
            Endianness::Big => {
                size_slice.write_u32::<BigEndian>(size)?;
            }
            Endianness::Little => {
                size_slice.write_u32::<LittleEndian>(size)?;
            }

            _ => unreachable!(),
        }

        Ok(())
    }

    fn write(mut self, root: &Root, version: u32) -> GFBSONResult<Vec<u8>> {
        // collect all strings
        self.collect_strings(&root.object_node);

        // write header
        self.buffer.write_all(b"BSON")?;
        self.write_u32(version)?; // version?
        self.write_u32(0x10)?; // root node offset
        self.write_u32(PLACEHOLDER_VALUE)?; // size placeholder

        // write root node
        self.write_u32(RawNodeType::Root as u32)?;
        let root_size_pos = self.buffer.len();
        self.write_u32(PLACEHOLDER_VALUE)?; // root size placeholder

        self.write_node(&root.object_node)?;

        // fill in root node information
        let after_root_pos = self.buffer.len();
        let root_size = (after_root_pos - root_size_pos - 4) as u32;
        let mut root_size_slice = &mut self.buffer[root_size_pos..root_size_pos + 4];

        match self.endian {
            Endianness::Big => {
                root_size_slice.write_u32::<BigEndian>(root_size)?;
            }

            Endianness::Little => {
                root_size_slice.write_u32::<LittleEndian>(root_size)?;
            }

            _ => unreachable!(),
        }

        // write string table
        self.write_u32(RawNodeType::StringTable as u32)?;
        self.write_u32(self.string_order.len() as u32 * 8)?;

        let mut current_bank_offset = 0;
        let mut string_bank_data = Vec::new();

        for string in self.string_order.clone().iter() {
            let bytes = string.as_bytes();
            let len = (bytes.len() + 1) as u32; // +1 for null terminator
            self.write_u32(current_bank_offset)?;
            self.write_u32(len)?;

            string_bank_data.extend_from_slice(bytes);
            string_bank_data.push(0); // null terminator
            current_bank_offset += len;
        }

        // make sure the string bank is 4-byte aligned
        while string_bank_data.len() % 4 != 0 {
            string_bank_data.push(0);
        }

        // string bank
        self.write_u32(RawNodeType::StringBank as u32)?;
        self.write_u32(string_bank_data.len() as u32)?;
        self.buffer.extend_from_slice(&string_bank_data);

        // EOF
        self.write_u32(RawNodeType::EndOfFile as u32)?;
        self.write_u32(0)?;

        // fill in size
        let total_size = self.buffer.len() as u32;
        let mut total_size_slice = &mut self.buffer[0xC..0x10];

        match self.endian {
            Endianness::Big => {
                total_size_slice.write_u32::<BigEndian>(total_size)?;
            }

            Endianness::Little => {
                total_size_slice.write_u32::<LittleEndian>(total_size)?;
            }

            _ => unreachable!(),
        }

        Ok(self.buffer)
    }
}

/// Writes a BSON file from the root node.
/// Using `Endianness::Auto` is not valid, and will instead be treated as `Endianness::Big`.
pub fn write(root: &Root, version: u32, endian: Endianness) -> GFBSONResult<Vec<u8>> {
    if matches!(endian, Endianness::Auto) {
        eprintln!("Cannot use automatic endianness. Writing in big endian...");
        Writer::new(Endianness::Big)
    } else {
        Writer::new(endian)
    }
    .write(root, version)
}

#[cfg(feature = "json")]
pub fn to_json(root: &Root, pretty: bool) -> serde_json::Result<String> {
    if pretty {
        serde_json::to_string_pretty(root)
    } else {
        serde_json::to_string(root)
    }
}

/* Intermediate Structures */

#[derive(Debug)]
struct StringEntry {
    offset: u32,
    length: u32,
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum RawNodeType {
    Root = 300,
    Object = 301,
    Array = 302,
    Integer = 303,
    Float = 304,
    String = 305,
    Bool = 306,
    StringTable = 400,
    StringBank = 500,
    EndOfFile = 900,
}

impl TryFrom<u32> for RawNodeType {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            300 => Ok(RawNodeType::Root),
            301 => Ok(RawNodeType::Object),
            302 => Ok(RawNodeType::Array),
            303 => Ok(RawNodeType::Integer),
            304 => Ok(RawNodeType::Float),
            305 => Ok(RawNodeType::String),
            306 => Ok(RawNodeType::Bool),
            400 => Ok(RawNodeType::StringTable),
            500 => Ok(RawNodeType::StringBank),
            900 => Ok(RawNodeType::EndOfFile),
            _ => Err(value),
        }
    }
}

/* Data */

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct Root {
    pub object_node: Node,
}

#[cfg(feature = "serde")]
impl Serialize for Root {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // don't show the object node's name
        self.object_node.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl Root {
    pub fn from_json_value(value: serde_json::Value) -> Self {
        Self {
            object_node: Node::from_json_pair("".to_string(), value),
        }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "lowercase"))]
pub enum Node {
    /// An object containing other nodes.
    Object { key: String, nodes: Vec<Node> },
    /// An array of other nodes.
    Array { key: String, nodes: Vec<Node> },
    /// A 32-bit signed integer.
    Integer { key: String, value: i32 },
    /// A 32-bit floating point value.
    Float { key: String, value: f32 },
    /// A string.
    String { key: String, value: String },
    /// A bool.
    Bool { key: String, value: bool },
}

#[cfg(feature = "serde")]
impl Serialize for Node {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Node::Object { nodes, .. } => {
                let mut map = serializer.serialize_map(Some(nodes.len()))?;
                for node in nodes {
                    map.serialize_entry(node.key(), node)?;
                }
                map.end()
            }
            Node::Array { nodes, .. } => {
                let mut seq = serializer.serialize_seq(Some(nodes.len()))?;
                for node in nodes {
                    seq.serialize_element(node)?;
                }
                seq.end()
            }
            Node::Integer { value, .. } => serializer.serialize_i32(*value),
            Node::Float { value, .. } => serializer.serialize_f32(*value),
            Node::String { value, .. } => serializer.serialize_str(value),
            Node::Bool { value, .. } => serializer.serialize_bool(*value),
        }
    }
}

#[cfg(feature = "serde")]
impl Node {
    pub fn key(&self) -> &str {
        match self {
            Node::Object { key, .. }
            | Node::Array { key, .. }
            | Node::Integer { key, .. }
            | Node::Float { key, .. }
            | Node::String { key, .. }
            | Node::Bool { key, .. } => key,
        }
    }

    fn from_json_pair(key: String, value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Object(map) => {
                let nodes = map
                    .into_iter()
                    .map(|(k, v)| Node::from_json_pair(k, v))
                    .collect();
                Node::Object { key, nodes }
            }
            serde_json::Value::Array(vec) => {
                let nodes = vec
                    .into_iter()
                    .map(|v| Node::from_json_pair("".to_string(), v)) // arrays get empty keys
                    .collect();
                Node::Array { key, nodes }
            }
            serde_json::Value::String(s) => Node::String { key, value: s },
            serde_json::Value::Bool(b) => Node::Bool { key, value: b },
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Node::Integer {
                        key,
                        value: i as i32,
                    }
                } else {
                    Node::Float {
                        key,
                        value: n.as_f64().unwrap_or(0.0) as f32,
                    }
                }
            }
            serde_json::Value::Null => Node::String {
                key,
                value: "null".to_string(),
            },
        }
    }
}
