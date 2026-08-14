//! Windows PE (Portable Executable) image construction.
//!
//! Builds a complete 64-bit PE executable image from a code section, an
//! initialized-data section, and a relocation section — no external
//! assembler or linker is involved. The image is a standard PE32+ console
//! executable:
//!
//! - **`.text`** — the machine code, with the entry-point stub first;
//! - **`.data`** — the module bindings (8 bytes each, little-endian);
//!   emitted only when the program has bindings, so an empty program never
//!   produces a zero-sized section;
//! - **`.reloc`** — a base-relocation block. The emitted code uses only
//!   relative addressing (RIP-relative data references and `rel32`
//!   control transfers), so nothing needs a fixup when the loader moves
//!   the image; the block carries absolute (no-op) entries so the image is
//!   formally relocatable under mandatory ASLR.
//!
//! The image has no imports: the entry point returns the `main` result and
//! the loader turns that return value into the process exit code.
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

fn align(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

/// A section to be written: its name, contents, virtual size, and
/// characteristics.
struct Section {
    name: &'static [u8],
    contents: Vec<u8>,
    characteristics: u32,
}

/// The layout of the sections, computed from their contents.
struct Layout {
    text_rva: u32,
    data_rva: u32,
    reloc_rva: u32,
    text_file_offset: u32,
    data_file_offset: u32,
    reloc_file_offset: u32,
    size_of_image: u32,
}

/// Computes the virtual addresses and file offsets of the three sections.
/// `.data` is skipped when empty, so its RVA is never allocated; `.text`
/// and `.reloc` always exist. Each section's RVA is aligned to the section
/// alignment and starts after the previous section's virtual extent, so no
/// two sections ever share an RVA.
fn layout(text_size: u32, data_size: u32, reloc_size: u32) -> Layout {
    let data_rva = if data_size == 0 {
        0
    } else {
        align(TEXT_RVA + text_size, SECTION_ALIGNMENT)
    };
    let reloc_rva = if data_size == 0 {
        align(TEXT_RVA + text_size, SECTION_ALIGNMENT)
    } else {
        align(data_rva + data_size, SECTION_ALIGNMENT)
    };
    let text_file_offset = SIZE_OF_HEADERS;
    let data_file_offset = align(text_file_offset + text_size, FILE_ALIGNMENT);
    let reloc_file_offset = align(data_file_offset + data_size, FILE_ALIGNMENT);
    let size_of_image = align(reloc_rva + reloc_size, SECTION_ALIGNMENT);
    Layout {
        text_rva: TEXT_RVA,
        data_rva,
        reloc_rva,
        text_file_offset,
        data_file_offset,
        reloc_file_offset,
        size_of_image,
    }
}

/// Builds a complete PE image.
///
/// - `code` — the `.text` section contents (the entry-point stub first);
/// - `data` — the `.data` section contents (the module bindings); may be
///   empty, in which case no `.data` section is emitted;
/// - `reloc` — the `.reloc` section contents (a base-relocation block);
/// - `entry_offset` — the offset of the entry point within `code`.
pub(crate) fn build(code: &[u8], data: &[u8], reloc: &[u8], entry_offset: u32) -> Vec<u8> {
    let layout = layout(code.len() as u32, data.len() as u32, reloc.len() as u32);
    let mut sections = Vec::with_capacity(3);
    sections.push(Section {
        name: b".text",
        contents: code.to_vec(),
        characteristics: 0x6000_0020, // CODE | EXECUTE | READ
    });
    if !data.is_empty() {
        sections.push(Section {
            name: b".data",
            contents: data.to_vec(),
            characteristics: 0xC000_0040, // INITIALIZED_DATA | READ | WRITE
        });
    }
    sections.push(Section {
        name: b".reloc",
        contents: reloc.to_vec(),
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
    let data_file_size = align(data.len() as u32, FILE_ALIGNMENT);
    let reloc_file_size = align(reloc.len() as u32, FILE_ALIGNMENT);
    image.extend_from_slice(&(data_file_size + reloc_file_size).to_le_bytes()); // SizeOfInitializedData
    image.extend_from_slice(&0u32.to_le_bytes()); // SizeOfUninitializedData
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
    // Data directories (16 × 8 bytes); only the base-relocation directory
    // is present.
    for index in 0..16 {
        if index == 5 {
            image.extend_from_slice(&layout.reloc_rva.to_le_bytes());
            image.extend_from_slice(&(reloc.len() as u32).to_le_bytes());
        } else {
            image.extend_from_slice(&0u32.to_le_bytes());
            image.extend_from_slice(&0u32.to_le_bytes());
        }
    }

    // ------------------------------------------------------------------
    // Section headers (40 bytes each).
    // ------------------------------------------------------------------
    for section in &sections {
        let is_text = section.name == b".text";
        let is_data = section.name == b".data";
        let virtual_address = if is_text {
            layout.text_rva
        } else if is_data {
            layout.data_rva
        } else {
            layout.reloc_rva
        };
        let file_offset = if is_text {
            layout.text_file_offset
        } else if is_data {
            layout.data_file_offset
        } else {
            layout.reloc_file_offset
        };
        write_section_header(
            &mut image,
            section.name,
            section.contents.len() as u32,
            virtual_address,
            align(section.contents.len() as u32, FILE_ALIGNMENT),
            file_offset,
            section.characteristics,
        );
    }

    // ------------------------------------------------------------------
    // Section data. Each section's raw data is padded to its file-aligned
    // size so the file never ends inside a section's declared raw range
    // (a truncated section makes the loader reject the image).
    // ------------------------------------------------------------------
    for section in &sections {
        let file_offset = if section.name == b".text" {
            layout.text_file_offset
        } else if section.name == b".data" {
            layout.data_file_offset
        } else {
            layout.reloc_file_offset
        };
        image.resize(file_offset as usize, 0);
        image.extend_from_slice(&section.contents);
        image.resize(
            file_offset as usize + align(section.contents.len() as u32, FILE_ALIGNMENT) as usize,
            0,
        );
    }

    image
}

fn write_section_header(
    image: &mut Vec<u8>,
    name: &[u8],
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
    characteristics: u32,
) {
    debug_assert!(name.len() <= 8, "section names are at most 8 bytes");
    image.extend_from_slice(name);
    image.resize(image.len() + (8 - name.len()), 0);
    image.extend_from_slice(&virtual_size.to_le_bytes());
    image.extend_from_slice(&virtual_address.to_le_bytes());
    image.extend_from_slice(&size_of_raw_data.to_le_bytes());
    image.extend_from_slice(&pointer_to_raw_data.to_le_bytes());
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
