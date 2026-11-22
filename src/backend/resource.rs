//! Windows PE resources: an application icon and version information.
//!
//! Explorer, the taskbar and the file Properties dialog read the icon and the
//! version strings of an executable from its `.rsrc` section. MINK's backend
//! writes the executable itself and has no linker or resource compiler to lean
//! on, so this module builds that section directly and appends it to a finished
//! PE image.
//!
//! The same code branches `mink build` output and `mink.exe`: [`embed`] takes any
//! 64-bit PE that has no resources yet, adds a `.rsrc` section after the last
//! existing one and updates the headers that describe the image.
//!
//! What is written:
//!
//! * one `RT_ICON` resource per frame of the supplied `.ico`, and the
//!   `RT_GROUP_ICON` directory that ties them together as icon number 1;
//! * one `RT_VERSION` resource holding a fixed file/product version and a
//!   `StringFileInfo` table (English, Unicode code page).

use std::fmt;

/// The MINK application icon, shipped with the compiler so every program it
/// builds can carry it without reading anything from disk.
pub const DEFAULT_ICON: &[u8] = include_bytes!("../../npm/mink/windows/mink-app.ico");

const FILE_ALIGNMENT_DEFAULT: u32 = 0x200;
const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const RT_VERSION: u32 = 16;
/// English (United States). Used for every resource.
const LANG_EN_US: u32 = 0x0409;
/// `.rsrc`: initialized data, readable.
const RSRC_CHARACTERISTICS: u32 = 0x4000_0040;

/// A failure while building or embedding resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceError {
    /// The icon file is not a usable `.ico`.
    BadIcon(String),
    /// The image is not a 64-bit PE this module can extend.
    BadImage(String),
    /// The image already has resources; replacing them is not supported.
    AlreadyHasResources,
    /// There is no room left in the PE headers for another section.
    NoHeaderRoom,
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadIcon(why) => write!(f, "the icon is not a valid .ico file: {why}"),
            Self::BadImage(why) => write!(f, "not a 64-bit Windows executable this tool can extend: {why}"),
            Self::AlreadyHasResources => {
                write!(f, "the executable already has resources; MINK only adds them to one that has none")
            }
            Self::NoHeaderRoom => write!(f, "the executable's headers have no room for another section"),
        }
    }
}

impl std::error::Error for ResourceError {}

/// Version information shown on the Details tab of the file Properties dialog.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionInfo {
    /// File version as four numbers, for example `[1, 0, 5, 0]`.
    pub file_version: [u16; 4],
    /// Product version as four numbers.
    pub product_version: [u16; 4],
    /// Named strings such as `ProductName` or `FileDescription`. Empty values
    /// are skipped.
    pub strings: Vec<(String, String)>,
}

impl VersionInfo {
    /// Parses `1.2.3` (up to four numeric parts) into the four-number form.
    /// A part that is not a number, or a suffix such as `-beta`, counts as 0 from
    /// that point, so `1.0.5-rc1` becomes `[1, 0, 5, 0]`.
    pub fn parse_version(text: &str) -> [u16; 4] {
        let mut parts = [0u16; 4];
        for (index, piece) in text.split('.').take(4).enumerate() {
            let digits: String = piece.chars().take_while(|c| c.is_ascii_digit()).collect();
            parts[index] = digits.parse().unwrap_or(0);
            if digits.len() != piece.len() {
                break;
            }
        }
        parts
    }
}

/// What to put into an executable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResourceSpec {
    /// The bytes of an `.ico` file, if the executable should carry an icon.
    pub icon: Option<Vec<u8>>,
    /// Version information, if any.
    pub version: Option<VersionInfo>,
}

impl ResourceSpec {
    /// Whether there is nothing to write.
    pub fn is_empty(&self) -> bool {
        self.icon.is_none() && self.version.is_none()
    }
}

// ---------------------------------------------------------------------------
// Little-endian helpers
// ---------------------------------------------------------------------------

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn align(value: u32, to: u32) -> u32 {
    value.div_ceil(to) * to
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

/// One image inside an `.ico` file.
struct IconFrame {
    width: u8,
    height: u8,
    color_count: u8,
    planes: u16,
    bit_count: u16,
    data: Vec<u8>,
}

fn parse_ico(ico: &[u8]) -> Result<Vec<IconFrame>, ResourceError> {
    let bad = |why: &str| ResourceError::BadIcon(why.to_string());
    if ico.len() < 6 || u16_at(ico, 0) != Some(0) || u16_at(ico, 2) != Some(1) {
        return Err(bad("the header is not an icon directory"));
    }
    let count = u16_at(ico, 4).unwrap_or(0) as usize;
    if count == 0 {
        return Err(bad("it contains no images"));
    }
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let at = 6 + index * 16;
        let entry = ico.get(at..at + 16).ok_or_else(|| bad("the directory is cut short"))?;
        let size = u32_at(entry, 8).unwrap_or(0) as usize;
        let offset = u32_at(entry, 12).unwrap_or(0) as usize;
        let data = offset
            .checked_add(size)
            .and_then(|end| ico.get(offset..end))
            .ok_or_else(|| bad("an image runs past the end of the file"))?;
        if size == 0 {
            return Err(bad("an image is empty"));
        }
        frames.push(IconFrame {
            width: entry[0],
            height: entry[1],
            color_count: entry[2],
            planes: u16_at(entry, 4).unwrap_or(1),
            bit_count: u16_at(entry, 6).unwrap_or(32),
            data: data.to_vec(),
        });
    }
    Ok(frames)
}

/// The `RT_GROUP_ICON` directory: like the `.ico` directory, but each entry
/// names a separate `RT_ICON` resource by id instead of a file offset.
fn group_icon_directory(frames: &[IconFrame]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + frames.len() * 14);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    for (index, frame) in frames.iter().enumerate() {
        out.push(frame.width);
        out.push(frame.height);
        out.push(frame.color_count);
        out.push(0);
        out.extend_from_slice(&frame.planes.to_le_bytes());
        out.extend_from_slice(&frame.bit_count.to_le_bytes());
        out.extend_from_slice(&(frame.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&((index + 1) as u16).to_le_bytes());
    }
    out
}

// ---------------------------------------------------------------------------
// Version information
// ---------------------------------------------------------------------------

fn utf16z(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2 + 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn pad4(bytes: &mut Vec<u8>) {
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
}

/// One `VS_VERSIONINFO`-style block: length, value length, type, a key, an
/// optional value and child blocks, with every part on a 4-byte boundary.
fn version_block(key: &str, kind: u16, value: &[u8], value_len: u16, children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut block = vec![0u8; 6];
    block.extend(utf16z(key));
    pad4(&mut block);
    block.extend_from_slice(value);
    for child in children {
        pad4(&mut block);
        block.extend(child);
    }
    let length = block.len() as u16;
    put_u16(&mut block, 0, length);
    put_u16(&mut block, 2, value_len);
    put_u16(&mut block, 4, kind);
    block
}

fn version_resource(info: &VersionInfo) -> Vec<u8> {
    let mut fixed = Vec::with_capacity(52);
    let words = |v: [u16; 4]| ((v[0] as u32) << 16 | v[1] as u32, (v[2] as u32) << 16 | v[3] as u32);
    let (file_ms, file_ls) = words(info.file_version);
    let (product_ms, product_ls) = words(info.product_version);
    for dword in [
        0xFEEF_04BDu32, // signature
        0x0001_0000,    // structure version
        file_ms,
        file_ls,
        product_ms,
        product_ls,
        0x3F,        // flags mask
        0,           // flags
        0x0004_0004, // VOS_NT_WINDOWS32
        1,           // VFT_APP
        0,           // subtype
        0,           // date
        0,
    ] {
        fixed.extend_from_slice(&dword.to_le_bytes());
    }

    // Windows reads the string table only when it carries FileVersion and
    // ProductVersion (a table without them showed up empty in the file
    // Properties dialog), so they are always present: the text the caller gave,
    // or the four numbers written out.
    let dotted = |v: [u16; 4]| format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3]);
    let mut all: Vec<(String, String)> = info.strings.clone();
    for (name, numbers) in [
        ("FileVersion", info.file_version),
        ("ProductVersion", info.product_version),
    ] {
        if !all.iter().any(|(n, v)| n == name && !v.is_empty()) {
            all.push((name.to_string(), dotted(numbers)));
        }
    }
    let strings: Vec<Vec<u8>> = all
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(name, value)| {
            let data = utf16z(value);
            let chars = (data.len() / 2) as u16;
            version_block(name, 1, &data, chars, Vec::new())
        })
        .collect();
    // 0409 = English (US), 04B0 = Unicode code page.
    let table = version_block("040904B0", 1, &[], 0, strings);
    let string_file_info = version_block("StringFileInfo", 1, &[], 0, vec![table]);
    let translation = version_block("Translation", 0, &0x04B0_0409u32.to_le_bytes(), 4, Vec::new());
    let var_file_info = version_block("VarFileInfo", 1, &[], 0, vec![translation]);
    version_block("VS_VERSION_INFO", 0, &fixed, 52, vec![string_file_info, var_file_info])
}

// ---------------------------------------------------------------------------
// The resource directory tree
// ---------------------------------------------------------------------------

/// One resource: a type, an id, and its bytes.
struct Leaf {
    kind: u32,
    id: u32,
    data: Vec<u8>,
}

/// Lays out the three-level resource tree (type, id, language) followed by the
/// data, as the `.rsrc` section stored at virtual address `rva`.
fn build_tree(rva: u32, mut leaves: Vec<Leaf>) -> Vec<u8> {
    leaves.sort_by_key(|leaf| (leaf.kind, leaf.id));
    let mut kinds: Vec<u32> = leaves.iter().map(|leaf| leaf.kind).collect();
    kinds.dedup();

    // Sizes: one directory header is 16 bytes and each entry 8.
    let root_size = 16 + kinds.len() * 8;
    let type_dirs_size: usize = kinds
        .iter()
        .map(|kind| 16 + leaves.iter().filter(|l| l.kind == *kind).count() * 8)
        .sum();
    let lang_dirs_size = leaves.len() * (16 + 8);
    let entries_size = leaves.len() * 16;
    let data_start = root_size + type_dirs_size + lang_dirs_size + entries_size;

    let mut out: Vec<u8> = Vec::new();
    // Offsets of the pieces, computed in the same order they are written.
    let mut type_dir_offsets = Vec::new();
    let mut cursor = root_size;
    for kind in &kinds {
        type_dir_offsets.push(cursor);
        cursor += 16 + leaves.iter().filter(|l| l.kind == *kind).count() * 8;
    }
    let lang_dirs_start = cursor;
    let entries_start = lang_dirs_start + lang_dirs_size;

    let dir_header = |out: &mut Vec<u8>, id_entries: usize| {
        out.extend_from_slice(&[0u8; 12]);
        out.extend_from_slice(&0u16.to_le_bytes()); // named entries
        out.extend_from_slice(&(id_entries as u16).to_le_bytes());
    };
    let subdir_entry = |out: &mut Vec<u8>, id: u32, offset: usize| {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(0x8000_0000u32 | offset as u32).to_le_bytes());
    };

    // Root: one entry per resource type.
    dir_header(&mut out, kinds.len());
    for (index, kind) in kinds.iter().enumerate() {
        subdir_entry(&mut out, *kind, type_dir_offsets[index]);
    }
    // Type directories: one entry per resource id, pointing at its language directory.
    let mut leaf_index = 0usize;
    for kind in &kinds {
        let members: Vec<&Leaf> = leaves.iter().filter(|l| l.kind == *kind).collect();
        dir_header(&mut out, members.len());
        for _ in &members {
            subdir_entry(&mut out, leaves[leaf_index].id, lang_dirs_start + leaf_index * 24);
            leaf_index += 1;
        }
    }
    // Language directories: a single language per resource, pointing at a data entry.
    for index in 0..leaves.len() {
        dir_header(&mut out, 1);
        out.extend_from_slice(&LANG_EN_US.to_le_bytes());
        out.extend_from_slice(&((entries_start + index * 16) as u32).to_le_bytes());
    }
    // Data entries, then the data itself, each blob on a 4-byte boundary.
    let mut blob_offset = data_start;
    let mut blobs: Vec<u8> = Vec::new();
    for leaf in &leaves {
        out.extend_from_slice(&(rva + blob_offset as u32).to_le_bytes());
        out.extend_from_slice(&(leaf.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // code page
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        blobs.extend_from_slice(&leaf.data);
        let padded = align(leaf.data.len() as u32, 4) as usize;
        blobs.resize(blobs.len() + (padded - leaf.data.len()), 0);
        blob_offset += padded;
    }
    debug_assert_eq!(out.len(), data_start);
    out.extend(blobs);
    out
}

/// Builds the bytes of the `.rsrc` section for `spec`, to be placed at
/// virtual address `rva`.
pub fn build_section(spec: &ResourceSpec, rva: u32) -> Result<Vec<u8>, ResourceError> {
    let mut leaves = Vec::new();
    if let Some(icon) = &spec.icon {
        let frames = parse_ico(icon)?;
        leaves.push(Leaf {
            kind: RT_GROUP_ICON,
            id: 1,
            data: group_icon_directory(&frames),
        });
        for (index, frame) in frames.into_iter().enumerate() {
            leaves.push(Leaf {
                kind: RT_ICON,
                id: (index + 1) as u32,
                data: frame.data,
            });
        }
    }
    if let Some(version) = &spec.version {
        leaves.push(Leaf {
            kind: RT_VERSION,
            id: 1,
            data: version_resource(version),
        });
    }
    Ok(build_tree(rva, leaves))
}

// ---------------------------------------------------------------------------
// Extending a PE image
// ---------------------------------------------------------------------------

const SECTION_HEADER_SIZE: usize = 40;

/// Returns a copy of the 64-bit PE image `pe` with `spec` added as a `.rsrc`
/// section.
///
/// The image must have no resources yet. The new section goes after the last
/// existing one (and after any data past the last section), so no existing
/// address moves and the base relocations stay valid.
pub fn embed(pe: &[u8], spec: &ResourceSpec) -> Result<Vec<u8>, ResourceError> {
    let bad = |why: &str| ResourceError::BadImage(why.to_string());
    if spec.is_empty() {
        return Ok(pe.to_vec());
    }
    if pe.get(0..2) != Some(b"MZ") {
        return Err(bad("missing the MZ header"));
    }
    let pe_at = u32_at(pe, 0x3C).ok_or_else(|| bad("truncated DOS header"))? as usize;
    if pe.get(pe_at..pe_at + 4) != Some(b"PE\0\0") {
        return Err(bad("missing the PE signature"));
    }
    let coff = pe_at + 4;
    if u16_at(pe, coff) != Some(0x8664) {
        return Err(bad("only x86-64 images are supported"));
    }
    let section_count = u16_at(pe, coff + 2).ok_or_else(|| bad("truncated COFF header"))? as usize;
    let optional_size = u16_at(pe, coff + 16).ok_or_else(|| bad("truncated COFF header"))? as usize;
    let optional = coff + 20;
    if u16_at(pe, optional) != Some(0x20B) {
        return Err(bad("not a PE32+ image"));
    }
    let section_alignment = u32_at(pe, optional + 32).ok_or_else(|| bad("truncated optional header"))?;
    let file_alignment = u32_at(pe, optional + 36).ok_or_else(|| bad("truncated optional header"))?;
    let file_alignment = if file_alignment == 0 { FILE_ALIGNMENT_DEFAULT } else { file_alignment };
    let size_of_image = u32_at(pe, optional + 56).ok_or_else(|| bad("truncated optional header"))?;
    let size_of_headers = u32_at(pe, optional + 60).ok_or_else(|| bad("truncated optional header"))? as usize;
    let directory_count = u32_at(pe, optional + 108).ok_or_else(|| bad("truncated optional header"))? as usize;
    if directory_count < 3 {
        return Err(bad("the image has no resource directory slot"));
    }
    let resource_directory = optional + 112 + 2 * 8;
    if u32_at(pe, resource_directory + 4).unwrap_or(0) != 0 {
        return Err(ResourceError::AlreadyHasResources);
    }

    let table = optional + optional_size;
    let mut first_raw = u32::MAX as usize;
    for index in 0..section_count {
        let at = table + index * SECTION_HEADER_SIZE;
        let name = pe.get(at..at + 8).ok_or_else(|| bad("truncated section table"))?;
        if name.starts_with(b".rsrc") {
            return Err(ResourceError::AlreadyHasResources);
        }
        let raw_size = u32_at(pe, at + 16).unwrap_or(0) as usize;
        let raw_ptr = u32_at(pe, at + 20).unwrap_or(0) as usize;
        if raw_size > 0 && raw_ptr > 0 {
            first_raw = first_raw.min(raw_ptr);
        }
    }
    let new_header = table + section_count * SECTION_HEADER_SIZE;
    let room_end = size_of_headers.min(first_raw);
    if new_header + SECTION_HEADER_SIZE > room_end {
        return Err(ResourceError::NoHeaderRoom);
    }

    let rva = align(size_of_image, section_alignment);
    let section = build_section(spec, rva)?;
    let raw_size = align(section.len() as u32, file_alignment);
    let raw_ptr = align(pe.len() as u32, file_alignment);

    let mut out = pe.to_vec();
    out.resize(raw_ptr as usize, 0);
    out.extend_from_slice(&section);
    out.resize((raw_ptr + raw_size) as usize, 0);

    // The new section header.
    let header = &mut out[new_header..new_header + SECTION_HEADER_SIZE];
    header.fill(0);
    header[..5].copy_from_slice(b".rsrc");
    put_u32(header, 8, section.len() as u32);
    put_u32(header, 12, rva);
    put_u32(header, 16, raw_size);
    put_u32(header, 20, raw_ptr);
    put_u32(header, 36, RSRC_CHARACTERISTICS);

    // Headers that describe the whole image.
    put_u16(&mut out, coff + 2, (section_count + 1) as u16);
    let initialized = u32_at(&out, optional + 8).unwrap_or(0);
    put_u32(&mut out, optional + 8, initialized + raw_size);
    put_u32(&mut out, optional + 56, align(rva + section.len() as u32, section_alignment));
    put_u32(&mut out, resource_directory, rva);
    put_u32(&mut out, resource_directory + 4, section.len() as u32);
    // The checksum is optional for executables; 0 means "not computed".
    put_u32(&mut out, optional + 64, 0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid PE with one `.text` section, enough to extend.
    fn tiny_pe() -> Vec<u8> {
        let mut image = vec![0u8; 0x400];
        image[0..2].copy_from_slice(b"MZ");
        put_u32(&mut image, 0x3C, 0x80);
        image[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        put_u16(&mut image, coff, 0x8664);
        put_u16(&mut image, coff + 2, 1);
        put_u16(&mut image, coff + 16, 240);
        let optional = coff + 20;
        put_u16(&mut image, optional, 0x20B);
        put_u32(&mut image, optional + 32, 0x1000);
        put_u32(&mut image, optional + 36, 0x200);
        put_u32(&mut image, optional + 56, 0x2000);
        put_u32(&mut image, optional + 60, 0x400);
        put_u32(&mut image, optional + 108, 16);
        let table = optional + 240;
        image[table..table + 5].copy_from_slice(b".text");
        put_u32(&mut image, table + 8, 4);
        put_u32(&mut image, table + 12, 0x1000);
        put_u32(&mut image, table + 16, 0x200);
        put_u32(&mut image, table + 20, 0x400);
        image.resize(0x600, 0);
        image
    }

    /// Walks the resource tree and returns (type, id, bytes) for every leaf.
    fn read_leaves(section: &[u8], rva: u32) -> Vec<(u32, u32, Vec<u8>)> {
        let entries = |dir: usize| {
            let named = u16_at(section, dir + 12).unwrap() as usize;
            let ids = u16_at(section, dir + 14).unwrap() as usize;
            (0..named + ids)
                .map(|i| {
                    let at = dir + 16 + i * 8;
                    (u32_at(section, at).unwrap(), u32_at(section, at + 4).unwrap())
                })
                .collect::<Vec<_>>()
        };
        let mut out = Vec::new();
        for (kind, kind_target) in entries(0) {
            assert!(kind_target & 0x8000_0000 != 0, "types point at subdirectories");
            for (id, id_target) in entries((kind_target & 0x7FFF_FFFF) as usize) {
                assert!(id_target & 0x8000_0000 != 0);
                for (_lang, data_target) in entries((id_target & 0x7FFF_FFFF) as usize) {
                    assert!(data_target & 0x8000_0000 == 0, "languages point at data entries");
                    let entry = data_target as usize;
                    let data_rva = u32_at(section, entry).unwrap();
                    let size = u32_at(section, entry + 4).unwrap() as usize;
                    let start = (data_rva - rva) as usize;
                    out.push((kind, id, section[start..start + size].to_vec()));
                }
            }
        }
        out
    }

    #[test]
    fn the_shipped_icon_parses_into_seven_frames() {
        let frames = parse_ico(DEFAULT_ICON).expect("the default icon is valid");
        assert_eq!(frames.len(), 7);
        assert_eq!(frames.last().unwrap().width, 0, "a 256 px frame is stored as 0");
    }

    #[test]
    fn malformed_icons_are_refused() {
        assert!(matches!(parse_ico(&[]), Err(ResourceError::BadIcon(_))));
        assert!(matches!(parse_ico(&[0, 0, 1, 0, 0, 0]), Err(ResourceError::BadIcon(_))));
        assert!(matches!(parse_ico(&[0, 0, 2, 0, 1, 0]), Err(ResourceError::BadIcon(_))));
        let mut cut = DEFAULT_ICON.to_vec();
        cut.truncate(40);
        assert!(matches!(parse_ico(&cut), Err(ResourceError::BadIcon(_))));
    }

    #[test]
    fn version_numbers_parse_leniently() {
        assert_eq!(VersionInfo::parse_version("1.0.5"), [1, 0, 5, 0]);
        assert_eq!(VersionInfo::parse_version("1.2.3.4"), [1, 2, 3, 4]);
        assert_eq!(VersionInfo::parse_version("1.0.5-rc1"), [1, 0, 5, 0]);
        assert_eq!(VersionInfo::parse_version("garbage"), [0, 0, 0, 0]);
        assert_eq!(VersionInfo::parse_version(""), [0, 0, 0, 0]);
    }

    #[test]
    fn tree_holds_every_icon_frame_the_group_and_the_version() {
        let spec = ResourceSpec {
            icon: Some(DEFAULT_ICON.to_vec()),
            version: Some(VersionInfo {
                file_version: [1, 0, 5, 0],
                product_version: [1, 0, 5, 0],
                strings: vec![("ProductName".into(), "Demo".into())],
            }),
        };
        let section = build_section(&spec, 0x3000).unwrap();
        let leaves = read_leaves(&section, 0x3000);
        assert_eq!(leaves.iter().filter(|l| l.0 == RT_ICON).count(), 7);
        assert_eq!(leaves.iter().filter(|l| l.0 == RT_GROUP_ICON).count(), 1);
        assert_eq!(leaves.iter().filter(|l| l.0 == RT_VERSION).count(), 1);
        // Each frame's bytes are the same bytes the .ico carried.
        let frames = parse_ico(DEFAULT_ICON).unwrap();
        for (index, frame) in frames.iter().enumerate() {
            let leaf = leaves.iter().find(|l| l.0 == RT_ICON && l.1 == (index + 1) as u32).unwrap();
            assert_eq!(leaf.2, frame.data);
        }
        // The group directory names exactly those ids.
        let group = &leaves.iter().find(|l| l.0 == RT_GROUP_ICON).unwrap().2;
        assert_eq!(u16_at(group, 4), Some(7));
        for index in 0..7 {
            assert_eq!(u16_at(group, 6 + index * 14 + 12), Some((index + 1) as u16));
        }
    }

    #[test]
    fn version_block_is_aligned_and_self_describing() {
        let info = VersionInfo {
            file_version: [1, 2, 3, 4],
            product_version: [5, 6, 7, 8],
            strings: vec![
                ("FileDescription".into(), "A test".into()),
                ("Empty".into(), String::new()),
            ],
        };
        let block = version_resource(&info);
        assert_eq!(u16_at(&block, 0), Some(block.len() as u16), "the outer length covers the block");
        assert_eq!(u16_at(&block, 2), Some(52));
        // The key follows the 6-byte header.
        assert_eq!(&block[6..6 + 32], utf16z("VS_VERSION_INFO").as_slice());
        // The fixed info starts on a 4-byte boundary after the key and carries the signature.
        assert_eq!(u32_at(&block, 40), Some(0xFEEF_04BD));
        assert_eq!(u32_at(&block, 48), Some(0x0001_0002), "file version, high word");
        assert_eq!(u32_at(&block, 52), Some(0x0003_0004), "file version, low word");
        let text: Vec<u16> = block
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let as_string = String::from_utf16_lossy(&text);
        assert!(as_string.contains("FileDescription"));
        assert!(as_string.contains("A test"));
        assert!(!as_string.contains("Empty"), "empty values are skipped");
        assert!(as_string.contains("FileVersion") && as_string.contains("1.2.3.4"));
        assert!(as_string.contains("ProductVersion") && as_string.contains("5.6.7.8"));
    }

    #[test]
    fn embedding_adds_one_section_and_updates_the_headers() {
        let pe = tiny_pe();
        let spec = ResourceSpec {
            icon: Some(DEFAULT_ICON.to_vec()),
            version: None,
        };
        let out = embed(&pe, &spec).unwrap();
        let coff = 0x84;
        let optional = coff + 20;
        assert_eq!(u16_at(&out, coff + 2), Some(2));
        let table = optional + 240;
        let header = table + SECTION_HEADER_SIZE;
        assert_eq!(&out[header..header + 5], b".rsrc");
        let rva = u32_at(&out, header + 12).unwrap();
        assert_eq!(rva, 0x2000, "the section follows the old SizeOfImage");
        let raw_ptr = u32_at(&out, header + 20).unwrap() as usize;
        let raw_size = u32_at(&out, header + 16).unwrap() as usize;
        assert_eq!(raw_ptr % 0x200, 0);
        assert_eq!(out.len(), raw_ptr + raw_size);
        assert_eq!(u32_at(&out, optional + 128), Some(rva), "resource directory rva");
        assert!(u32_at(&out, optional + 132).unwrap() > 0, "resource directory size");
        let size_of_image = u32_at(&out, optional + 56).unwrap();
        assert!(size_of_image > rva && size_of_image % 0x1000 == 0);
        // The original bytes are untouched apart from the headers.
        assert_eq!(&out[0x400..0x600], &pe[0x400..0x600]);
    }

    #[test]
    fn an_image_that_already_has_resources_is_refused() {
        let pe = tiny_pe();
        let spec = ResourceSpec {
            icon: Some(DEFAULT_ICON.to_vec()),
            version: None,
        };
        let once = embed(&pe, &spec).unwrap();
        assert_eq!(embed(&once, &spec), Err(ResourceError::AlreadyHasResources));
    }

    #[test]
    fn non_pe_input_and_an_empty_spec_are_handled() {
        let spec = ResourceSpec {
            icon: Some(DEFAULT_ICON.to_vec()),
            version: None,
        };
        assert!(matches!(embed(b"not a pe", &spec), Err(ResourceError::BadImage(_))));
        let pe = tiny_pe();
        assert_eq!(embed(&pe, &ResourceSpec::default()).unwrap(), pe, "nothing to add");
        let mut wrong_machine = pe.clone();
        put_u16(&mut wrong_machine, 0x84, 0x014C);
        assert!(matches!(embed(&wrong_machine, &spec), Err(ResourceError::BadImage(_))));
    }

    #[test]
    fn a_bad_icon_fails_before_the_image_is_touched() {
        let spec = ResourceSpec {
            icon: Some(vec![1, 2, 3]),
            version: None,
        };
        assert!(matches!(embed(&tiny_pe(), &spec), Err(ResourceError::BadIcon(_))));
    }
}
