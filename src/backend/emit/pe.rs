//! Windows PE (Portable Executable) image construction.
//!
//! Builds a complete 64-bit PE executable image from a code section, an
//! initialized-data section, a zero-initialized `.bss` section, an import
//! section, and a relocation section — no external assembler or linker is
//! involved. The image is a standard PE32+ console executable:
//!
//! - **`.text`** — the machine code, with the entry-point stub first;
//! - **`.data`** — the module bindings (8 bytes each, little-endian);
//!   emitted only when the program has bindings, so an empty program never
//!   produces a zero-sized section;
//! - **`.bss`** — the runtime state: the heap arena, the liveness table,
//!   and the runtime's globals. Zero-initialized by the loader (the file
//!   carries no bytes), so every image starts with a deterministic
//!   runtime state;
//! - **`.idata`** — the import directory: `kernel32.dll`'s `GetStdHandle`
//!   and `WriteFile`, which the runtime's output and error paths use. The
//!   import directory is present in every image;
//! - **`.reloc`** — a base-relocation block. The emitted code uses only
//!   relative addressing (RIP-relative data references and `rel32`
//!   control transfers), so nothing needs a fixup when the loader moves
//!   the image; the block carries absolute (no-op) entries so the image is
//!   formally relocatable under mandatory ASLR.
//!
//! The entry point runs the runtime's exit service, which terminates the
//! process by returning — the loader turns that return value into the
//! process exit code. Runtime errors are written to stderr through the
//! imported `WriteFile` before terminating.
//!
//! Layout constants follow the standard PE conventions (image base
//! `0x140000000`, 4K section alignment, 512-byte file alignment, 0x400-byte
//! headers).

/// File alignment: raw section data starts at multiples of this.
const FILE_ALIGNMENT: u32 = 0x200;
/// Section alignment: virtual addresses are multiples of this.
const SECTION_ALIGNMENT: u32 = 0x1000;
/// Size of the headers (DOS + PE + COFF + optional + section headers).
const SIZE_OF_HEADERS: u32 = 0x400;
/// Preferred image base.
const IMAGE_BASE: u64 = 0x1_4000_0000;
/// RVA of the first section (.text).
pub(crate) const TEXT_RVA: u32 = SECTION_ALIGNMENT;
/// Offset of the import lookup table within the `.idata` section.
const ILT_OFFSET: u32 = 40;
/// Offset of the import address table within the `.idata` section.
pub(crate) const IAT_OFFSET: u32 = 64;
/// Total size of the `.idata` section.
pub(crate) const IDATA_SIZE: u32 = 128;
/// Size of the import directory (two descriptors).
const IMPORT_DIRECTORY_SIZE: u32 = 40;

fn align(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

/// The layout of the sections, computed from their contents.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) text_rva: u32,
    pub(crate) data_rva: u32,
    pub(crate) bss_rva: u32,
    pub(crate) idata_rva: u32,
    pub(crate) reloc_rva: u32,
    /// The virtual size of the `.bss` section (the runtime state).
    pub(crate) bss_size: u32,
    text_file_offset: u32,
    data_file_offset: u32,
    idata_file_offset: u32,
    reloc_file_offset: u32,
    size_of_image: u32,
}

/// Computes the virtual addresses and file offsets of the five sections.
/// `.data` is skipped when empty, so its RVA is never allocated; `.text`,
/// `.bss`, `.idata`, and `.reloc` always exist. Each section's RVA is
/// aligned to the section alignment and starts after the previous
/// section's virtual extent, so no two sections ever share an RVA. `.bss`
/// carries no raw data.
pub(crate) fn layout(
    text_size: u32,
    data_size: u32,
    bss_size: u32,
    idata_size: u32,
    reloc_size: u32,
) -> Layout {
    let data_rva = if data_size == 0 {
        0
    } else {
        align(TEXT_RVA + text_size, SECTION_ALIGNMENT)
    };
    let bss_rva = if data_rva == 0 {
        align(TEXT_RVA + text_size, SECTION_ALIGNMENT)
    } else {
        align(data_rva + data_size, SECTION_ALIGNMENT)
    };
    let idata_rva = align(bss_rva + bss_size, SECTION_ALIGNMENT);
    let reloc_rva = align(idata_rva + idata_size, SECTION_ALIGNMENT);
    let text_file_offset = SIZE_OF_HEADERS;
    let data_file_offset = align(text_file_offset + text_size, FILE_ALIGNMENT);
    let idata_file_offset = align(data_file_offset + data_size, FILE_ALIGNMENT);
    let reloc_file_offset = align(idata_file_offset + idata_size, FILE_ALIGNMENT);
    let size_of_image = align(reloc_rva + reloc_size, SECTION_ALIGNMENT);
    Layout {
        text_rva: TEXT_RVA,
        data_rva,
        bss_rva,
        idata_rva,
        reloc_rva,
        bss_size,
        text_file_offset,
        data_file_offset,
        idata_file_offset,
        reloc_file_offset,
        size_of_image,
    }
}

/// A section to be written: its name, contents (empty for `.bss`), virtual
/// size, raw size, file offset, and characteristics.
struct Section {
    name: &'static [u8],
    contents: Vec<u8>,
    virtual_size: u32,
    raw_size: u32,
    file_offset: u32,
    characteristics: u32,
}

/// Builds the `.idata` section: the import directory for `kernel32.dll`
/// with `GetStdHandle` (IAT entry 0) and `WriteFile` (IAT entry 1). All
/// RVAs are relative to `idata_rva`, so the section is position-independent.
pub(crate) fn build_idata(idata_rva: u32) -> Vec<u8> {
    let ilt_rva = idata_rva + ILT_OFFSET;
    let iat_rva = idata_rva + IAT_OFFSET;
    let getstdhandle_name = idata_rva + 88;
    let writefile_name = idata_rva + 104;
    let dll_name_rva = idata_rva + 116;

    let mut bytes = Vec::with_capacity(IDATA_SIZE as usize);
    // Import descriptor for kernel32.dll.
    bytes.extend_from_slice(&ilt_rva.to_le_bytes()); // OriginalFirstThunk (ILT)
    bytes.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
    bytes.extend_from_slice(&0u32.to_le_bytes()); // ForwarderChain
    bytes.extend_from_slice(&dll_name_rva.to_le_bytes()); // Name
    bytes.extend_from_slice(&iat_rva.to_le_bytes()); // FirstThunk (IAT)
    // Null descriptor terminating the import directory.
    bytes.extend_from_slice(&[0u8; 20]);
    // Import lookup table: two by-name entries (64-bit: the ordinal bit
    // clear, the low 32 bits the hint/name RVA), followed by a zero
    // terminator — the loader scans the table until it finds the zero
    // entry, so the table must end before any other data.
    bytes.extend_from_slice(&(getstdhandle_name as u64).to_le_bytes());
    bytes.extend_from_slice(&(writefile_name as u64).to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    // Import address table: initialized to the hint/name RVAs (64-bit
    // entries), followed by a zero terminator; the loader overwrites the
    // entries with the resolved addresses.
    bytes.extend_from_slice(&(getstdhandle_name as u64).to_le_bytes());
    bytes.extend_from_slice(&(writefile_name as u64).to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    // Hint/name entries.
    bytes.extend_from_slice(&0u16.to_le_bytes()); // hint
    bytes.extend_from_slice(b"GetStdHandle\0");
    bytes.resize(104, 0);
    bytes.extend_from_slice(&0u16.to_le_bytes()); // hint
    bytes.extend_from_slice(b"WriteFile\0");
    bytes.resize(116, 0);
    // DLL name.
    bytes.extend_from_slice(b"kernel32.dll\0");
    bytes.resize(IDATA_SIZE as usize, 0);
    debug_assert_eq!(bytes.len(), IDATA_SIZE as usize);
    bytes
}

/// Builds a complete PE image.
///
/// - `code` — the `.text` section contents (the entry-point stub first);
/// - `data` — the `.data` section contents (the module bindings); may be
///   empty, in which case no `.data` section is emitted;
/// - `layout` — the computed section layout;
/// - `idata` — the `.idata` section contents (the import directory);
/// - `reloc` — the `.reloc` section contents (a base-relocation block);
/// - `entry_offset` — the offset of the entry point within `code`.
pub(crate) fn build(
    code: &[u8],
    data: &[u8],
    layout: Layout,
    idata: &[u8],
    reloc: &[u8],
    entry_offset: u32,
) -> Vec<u8> {
    let bss_size = layout.bss_size;
    let mut sections = Vec::with_capacity(5);
    sections.push(Section {
        name: b".text",
        contents: code.to_vec(),
        virtual_size: code.len() as u32,
        raw_size: align(code.len() as u32, FILE_ALIGNMENT),
        file_offset: layout.text_file_offset,
        characteristics: 0x6000_0020, // CODE | EXECUTE | READ
    });
    if !data.is_empty() {
        sections.push(Section {
            name: b".data",
            contents: data.to_vec(),
            virtual_size: data.len() as u32,
            raw_size: align(data.len() as u32, FILE_ALIGNMENT),
            file_offset: layout.data_file_offset,
            characteristics: 0xC000_0040, // INITIALIZED_DATA | READ | WRITE
        });
    }
    sections.push(Section {
        name: b".bss",
        contents: Vec::new(),
        virtual_size: bss_size,
        raw_size: 0,
        file_offset: 0,
        characteristics: 0xC000_0080, // READ | WRITE | UNINITIALIZED_DATA
    });
    sections.push(Section {
        name: b".idata",
        contents: idata.to_vec(),
        virtual_size: idata.len() as u32,
        raw_size: align(idata.len() as u32, FILE_ALIGNMENT),
        file_offset: layout.idata_file_offset,
        characteristics: 0xC000_0040, // INITIALIZED_DATA | READ | WRITE
    });
    sections.push(Section {
        name: b".reloc",
        contents: reloc.to_vec(),
        virtual_size: reloc.len() as u32,
        raw_size: align(reloc.len() as u32, FILE_ALIGNMENT),
        file_offset: layout.reloc_file_offset,
        characteristics: 0x4200_0040, // INITIALIZED_DATA | READ | DISCARDABLE
    });

    let mut image = Vec::with_capacity(layout.size_of_image as usize);

    // ------------------------------------------------------------------
    // DOS header (64 bytes) + e_lfanew pointing at the PE signature.
    // ------------------------------------------------------------------
    image.extend_from_slice(b"MZ");
    image.resize(0x3C, 0);
    image.extend_from_slice(&0x80u32.to_le_bytes()); // e_lfanew
    image.resize(0x80, 0);

    // ------------------------------------------------------------------
    // PE signature + COFF header (20 bytes).
    // ------------------------------------------------------------------
    image.extend_from_slice(b"PE\0\0");
    image.extend_from_slice(&0x8664u16.to_le_bytes()); // Machine: x64
    image.extend_from_slice(&(sections.len() as u16).to_le_bytes()); // NumberOfSections
    image.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
    image.extend_from_slice(&0u32.to_le_bytes()); // PointerToSymbolTable
    image.extend_from_slice(&0u32.to_le_bytes()); // NumberOfSymbols
    image.extend_from_slice(&240u16.to_le_bytes()); // SizeOfOptionalHeader (PE32+)
    image.extend_from_slice(&0x0022u16.to_le_bytes()); // Characteristics: EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE

    // ------------------------------------------------------------------
    // Optional header, PE32+ (240 bytes).
    // ------------------------------------------------------------------
    image.extend_from_slice(&0x20Bu16.to_le_bytes()); // Magic: PE32+
    image.push(0); // MajorLinkerVersion
    image.push(0); // MinorLinkerVersion
    image.extend_from_slice(&align(code.len() as u32, FILE_ALIGNMENT).to_le_bytes()); // SizeOfCode
    let data_raw = if data.is_empty() {
        0
    } else {
        align(data.len() as u32, FILE_ALIGNMENT)
    };
    let idata_raw = align(idata.len() as u32, FILE_ALIGNMENT);
    let reloc_raw = align(reloc.len() as u32, FILE_ALIGNMENT);
    image.extend_from_slice(&(data_raw + idata_raw + reloc_raw).to_le_bytes()); // SizeOfInitializedData
    image.extend_from_slice(&align(bss_size, FILE_ALIGNMENT).to_le_bytes()); // SizeOfUninitializedData
    image.extend_from_slice(&(layout.text_rva + entry_offset).to_le_bytes()); // AddressOfEntryPoint
    image.extend_from_slice(&layout.text_rva.to_le_bytes()); // BaseOfCode
    image.extend_from_slice(&IMAGE_BASE.to_le_bytes()); // ImageBase
    image.extend_from_slice(&SECTION_ALIGNMENT.to_le_bytes()); // SectionAlignment
    image.extend_from_slice(&FILE_ALIGNMENT.to_le_bytes()); // FileAlignment
    image.extend_from_slice(&6u16.to_le_bytes()); // MajorOperatingSystemVersion
    image.extend_from_slice(&0u16.to_le_bytes()); // MinorOperatingSystemVersion
    image.extend_from_slice(&0u16.to_le_bytes()); // MajorImageVersion
    image.extend_from_slice(&0u16.to_le_bytes()); // MinorImageVersion
    image.extend_from_slice(&6u16.to_le_bytes()); // MajorSubsystemVersion
    image.extend_from_slice(&0u16.to_le_bytes()); // MinorSubsystemVersion
    image.extend_from_slice(&0u32.to_le_bytes()); // Win32VersionValue
    image.extend_from_slice(&layout.size_of_image.to_le_bytes()); // SizeOfImage
    image.extend_from_slice(&SIZE_OF_HEADERS.to_le_bytes()); // SizeOfHeaders
    image.extend_from_slice(&0u32.to_le_bytes()); // CheckSum
    image.extend_from_slice(&3u16.to_le_bytes()); // Subsystem: CONSOLE
    image.extend_from_slice(&(0x0020u16 | 0x0100u16).to_le_bytes()); // DllCharacteristics: DYNAMIC_BASE | NX_COMPAT
    image.extend_from_slice(&0x100000u64.to_le_bytes()); // SizeOfStackReserve
    image.extend_from_slice(&0x1000u64.to_le_bytes()); // SizeOfStackCommit
    image.extend_from_slice(&0x100000u64.to_le_bytes()); // SizeOfHeapReserve
    image.extend_from_slice(&0x1000u64.to_le_bytes()); // SizeOfHeapCommit
    image.extend_from_slice(&0u32.to_le_bytes()); // LoaderFlags
    image.extend_from_slice(&16u32.to_le_bytes()); // NumberOfRvaAndSizes
    // Data directories (16 × 8 bytes): the import directory (index 1) and
    // the base-relocation directory (index 5).
    for index in 0..16 {
        let (rva, size) = match index {
            1 => (layout.idata_rva, IMPORT_DIRECTORY_SIZE),
            5 => (layout.reloc_rva, reloc.len() as u32),
            _ => (0, 0),
        };
        image.extend_from_slice(&rva.to_le_bytes());
        image.extend_from_slice(&size.to_le_bytes());
    }

    // ------------------------------------------------------------------
    // Section headers (40 bytes each).
    // ------------------------------------------------------------------
    for section in &sections {
        let virtual_address = match section.name {
            b".text" => layout.text_rva,
            b".data" => layout.data_rva,
            b".bss" => layout.bss_rva,
            b".idata" => layout.idata_rva,
            _ => layout.reloc_rva,
        };
        write_section_header(
            &mut image,
            section.name,
            section.virtual_size,
            virtual_address,
            section.raw_size,
            section.file_offset,
            section.characteristics,
        );
    }

    // ------------------------------------------------------------------
    // Section data. Each section's raw data is padded to its file-aligned
    // size so the file never ends inside a section's declared raw range
    // (a truncated section makes the loader reject the image).
    // ------------------------------------------------------------------
    for section in &sections {
        if section.name == b".bss" {
            // No raw data; the loader zero-fills the virtual extent.
            continue;
        }
        image.resize(section.file_offset as usize, 0);
        image.extend_from_slice(&section.contents);
        image.resize(section.file_offset as usize + section.raw_size as usize, 0);
    }

    image
}

fn write_section_header(
    image: &mut Vec<u8>,
    name: &[u8],
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    file_offset: u32,
    characteristics: u32,
) {
    debug_assert!(name.len() <= 8, "section names are at most 8 bytes");
    image.extend_from_slice(name);
    image.resize(image.len() + (8 - name.len()), 0);
    image.extend_from_slice(&virtual_size.to_le_bytes());
    image.extend_from_slice(&virtual_address.to_le_bytes());
    image.extend_from_slice(&raw_size.to_le_bytes()); // SizeOfRawData
    let pointer_to_raw_data = if name == b".bss" { 0 } else { file_offset };
    image.extend_from_slice(&pointer_to_raw_data.to_le_bytes()); // PointerToRawData
    image.extend_from_slice(&0u32.to_le_bytes()); // PointerToRelocations
    image.extend_from_slice(&0u32.to_le_bytes()); // PointerToLinenumbers
    image.extend_from_slice(&0u16.to_le_bytes()); // NumberOfRelocations
    image.extend_from_slice(&0u16.to_le_bytes()); // NumberOfLinenumbers
    image.extend_from_slice(&characteristics.to_le_bytes());
}

/// The base-relocation block for the emitted code: one block on the `.text`
/// page carrying absolute (no-op) entries, making the image formally
/// relocatable. The code uses only relative addressing, so no fixups are
/// needed when the loader moves the image.
pub(crate) fn relocation_block(text_rva: u32) -> Vec<u8> {
    let mut block = Vec::with_capacity(12);
    block.extend_from_slice(&(text_rva & !0xFFF).to_le_bytes()); // Page RVA
    block.extend_from_slice(&12u32.to_le_bytes()); // Block size
    block.extend_from_slice(&0x0000u16.to_le_bytes()); // ABSOLUTE, offset 0
    block.extend_from_slice(&0x0001u16.to_le_bytes()); // ABSOLUTE, offset 1
    block
}
