use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Seek};

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
}

pub type GFBSONResult<T> = Result<T, GFBSONError>;

struct Reader<'a> {
    cursor: Cursor<&'a [u8]>,
    strings: Vec<String>,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
            strings: Vec::new(),
        }
    }

    fn read_u32(&mut self) -> GFBSONResult<u32> {
        Ok(self.cursor.read_u32::<BigEndian>()?)
    }

    fn read_i32(&mut self) -> GFBSONResult<i32> {
        Ok(self.cursor.read_i32::<BigEndian>()?)
    }

    fn read_f32(&mut self) -> GFBSONResult<f32> {
        Ok(self.cursor.read_f32::<BigEndian>()?)
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
        let magic = self.read_u32()?.to_be_bytes();

        if &magic != b"BSON" {
            return Err(GFBSONError::InvalidMagicError);
        }

        self.skip(4); // version?
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

pub fn read(data: &[u8]) -> GFBSONResult<Root> {
    Reader::new(data).read_root()
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
pub struct Root {
    pub object_node: Node,
}

#[derive(Debug)]
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
