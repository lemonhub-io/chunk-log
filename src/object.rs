//! Canonically encoded, content-addressed repository objects.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{bail, Context, Result};

const MAGIC: &[u8; 4] = b"CHLG";
const FORMAT_VERSION: u8 = 1;
const TAG_BLOB: u8 = 1;
const TAG_TREE_BRANCH: u8 = 2;
const TAG_TREE_LEAF: u8 = 3;
const TAG_COMMIT: u8 = 4;

/// A content hash (BLAKE3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(
    /// The raw hash bytes.
    pub [u8; 32],
);

impl Hash {
    /// Computes the content address of canonical object bytes.
    pub fn digest(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).into())
    }

    /// Verifies that `bytes` are the content addressed by this hash.
    pub fn verify(self, bytes: &[u8]) -> Result<()> {
        let actual = Self::digest(bytes);
        if actual != self {
            bail!("object integrity mismatch: expected {self}, computed {actual}");
        }
        Ok(())
    }
}

/// Chunk coordinates in a voxel world.
pub type ChunkCoords = (i32, i32);

/// A node in the persistent coordinate radix tree.
///
/// Coordinates are encoded as sixteen nibbles (the big-endian bytes of
/// `x`, followed by the big-endian bytes of `z`). Updating a coordinate
/// republishes only nodes on affected root-to-leaf paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeNode {
    /// A sparse radix node mapping a nibble (`0..=15`) to a child node.
    Branch(BTreeMap<u8, Hash>),
    /// A terminal coordinate-to-blob mapping.
    Leaf {
        /// The exact chunk coordinate represented by the path.
        coords: ChunkCoords,
        /// Address of the encoded [`Object::Blob`].
        blob: Hash,
    },
}

/// An immutable repository object.
///
/// Every variant has a distinct canonical tag, preventing a blob from
/// aliasing a tree or commit merely because their payload bytes happen to
/// be identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Object {
    /// An opaque chunk payload.
    Blob(Vec<u8>),
    /// A node in the persistent coordinate tree.
    Tree(TreeNode),
    /// A version commit referencing a root tree.
    Commit(Commit),
}

/// Metadata for a single commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Hash of the root tree node of this commit.
    pub tree: Hash,
    /// Hash of the parent commit, or `None` for the first commit.
    pub parent: Option<Hash>,
    /// Unix timestamp (seconds) of the commit.
    pub timestamp: u64,
    /// Human-readable commit message.
    pub message: String,
}

impl Object {
    /// Computes the content hash of this object.
    pub fn hash(&self) -> Hash {
        Hash::digest(&self.to_bytes())
    }

    /// Serializes this object using chunklog's versioned canonical format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(FORMAT_VERSION);
        match self {
            Self::Blob(payload) => {
                out.push(TAG_BLOB);
                push_len(&mut out, payload.len());
                out.extend_from_slice(payload);
            }
            Self::Tree(TreeNode::Branch(children)) => {
                out.push(TAG_TREE_BRANCH);
                let count = u16::try_from(children.len())
                    .expect("a radix branch can contain at most sixteen children");
                out.extend_from_slice(&count.to_be_bytes());
                for (&nibble, hash) in children {
                    assert!(nibble < 16, "radix child nibble must be below sixteen");
                    out.push(nibble);
                    out.extend_from_slice(&hash.0);
                }
            }
            Self::Tree(TreeNode::Leaf { coords, blob }) => {
                out.push(TAG_TREE_LEAF);
                out.extend_from_slice(&coords.0.to_be_bytes());
                out.extend_from_slice(&coords.1.to_be_bytes());
                out.extend_from_slice(&blob.0);
            }
            Self::Commit(commit) => {
                out.push(TAG_COMMIT);
                out.extend_from_slice(&commit.tree.0);
                match commit.parent {
                    Some(parent) => {
                        out.push(1);
                        out.extend_from_slice(&parent.0);
                    }
                    None => out.push(0),
                }
                out.extend_from_slice(&commit.timestamp.to_be_bytes());
                push_len(&mut out, commit.message.len());
                out.extend_from_slice(commit.message.as_bytes());
            }
        }
        out
    }

    /// Deserializes one complete canonical object.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(4)? != MAGIC {
            bail!("invalid chunklog object magic");
        }
        let version = cursor.byte()?;
        if version != FORMAT_VERSION {
            bail!("unsupported chunklog object format version {version}");
        }
        let object = match cursor.byte()? {
            TAG_BLOB => {
                let len = cursor.len()?;
                Self::Blob(cursor.take(len)?.to_vec())
            }
            TAG_TREE_BRANCH => {
                let count = cursor.u16()? as usize;
                if count > 16 {
                    bail!("tree branch contains {count} children; maximum is 16");
                }
                let mut children = BTreeMap::new();
                for _ in 0..count {
                    let nibble = cursor.byte()?;
                    if nibble >= 16 {
                        bail!("invalid radix child nibble {nibble}");
                    }
                    let hash = cursor.hash()?;
                    if children.insert(nibble, hash).is_some() {
                        bail!("duplicate radix child nibble {nibble}");
                    }
                }
                Self::Tree(TreeNode::Branch(children))
            }
            TAG_TREE_LEAF => {
                let x = cursor.i32()?;
                let z = cursor.i32()?;
                let blob = cursor.hash()?;
                Self::Tree(TreeNode::Leaf {
                    coords: (x, z),
                    blob,
                })
            }
            TAG_COMMIT => {
                let tree = cursor.hash()?;
                let parent = match cursor.byte()? {
                    0 => None,
                    1 => Some(cursor.hash()?),
                    flag => bail!("invalid commit parent flag {flag}"),
                };
                let timestamp = cursor.u64()?;
                let message_len = cursor.len()?;
                let message = std::str::from_utf8(cursor.take(message_len)?)
                    .context("commit message is not UTF-8")?
                    .to_owned();
                Self::Commit(Commit {
                    tree,
                    parent,
                    timestamp,
                    message,
                })
            }
            tag => bail!("unknown chunklog object tag {tag}"),
        };
        if !cursor.is_empty() {
            bail!("trailing bytes after canonical object");
        }
        Ok(object)
    }
}

fn push_len(out: &mut Vec<u8>, len: usize) {
    let len = u64::try_from(len).expect("object length exceeds u64");
    out.extend_from_slice(&len.to_be_bytes());
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .context("object length overflow")?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .context("truncated chunklog object")?;
        self.pos = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn len(&mut self) -> Result<usize> {
        usize::try_from(self.u64()?).context("object length does not fit this platform")
    }

    fn hash(&mut self) -> Result<Hash> {
        Ok(Hash(self.take(32)?.try_into().unwrap()))
    }

    fn is_empty(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Parses a 64-character lowercase or uppercase hexadecimal hash.
pub fn parse_hash(s: &str) -> Result<Hash> {
    if s.len() != 64 {
        bail!("invalid hash: {s}");
    }
    let mut hash = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
        hash[i] = u8::from_str_radix(std::str::from_utf8(chunk)?, 16)?;
    }
    Ok(Hash(hash))
}
