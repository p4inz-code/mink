//! Windows PE (Portable Executable) image construction.
//!
//! Builds a complete 64-bit PE executable image from a code section, an
//! initialized-data section, a zero-initialized `.bss` section, an import
//! section, and a relocation section — no external assembler or linker is
//! involved.

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

// ---------------------------------------------------------------------------
// Kernel32.dll imports (always present)
// ---------------------------------------------------------------------------

const KERNEL32_IMPORTS: &[&str] = &[
    "GetStdHandle",
    "WriteFile",
    "CreateFileA",
    "CloseHandle",
    "ReadFile",
    "GetFileAttributesA",
    "GetFileSize",
    "FindFirstFileA",
    "FindNextFileA",
    "FindClose",
    "CreateDirectoryA",
    "RemoveDirectoryA",
    "DeleteFileA",
    "MoveFileA",
    "CopyFileA",
    "GetCurrentDirectoryA",
    "SetCurrentDirectoryA",
    // --- Process (Session 59) ---
    "CreatePipe",
    "CreateProcessA",
    "GetExitCodeProcess",
    "WaitForSingleObject",
    "TerminateProcess",
    "GetCurrentProcessId",
    "SetHandleInformation",
    // --- Time (Session 60) ---
    "GetSystemTimeAsFileTime",
    "GetTickCount64",
    "QueryPerformanceCounter",
    "QueryPerformanceFrequency",
    // --- Environment (Session 65) ---
    "GetEnvironmentVariableA",
    "SetEnvironmentVariableA",
    "GetEnvironmentStringsA",
    "FreeEnvironmentStringsA",
    // --- Networking dynamic loading (Session 67) ---
    "LoadLibraryA",
    "GetProcAddress",
];

const K32_COUNT: u32 = KERNEL32_IMPORTS.len() as u32;

// ---------------------------------------------------------------------------
// Offsets
// ---------------------------------------------------------------------------

const ILT_OFFSET: u32 = 40;
/// IAT offset: after ILT entries + null
pub(crate) const IAT_OFFSET: u32 = ILT_OFFSET + (K32_COUNT + 1) * 8;
pub(crate) const IDATA_SIZE: u32 = 1536;

/// Size of the import directory (1 descriptor + null = 40 bytes).
const IMPORT_DIR_SIZE: u32 = 40;

fn align(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

/// IAT indices for kernel32 functions.
pub(crate) mod iat {
    pub const GET_STD_HANDLE: u32 = 0;
    pub const WRITE_FILE: u32 = 1;
    pub const CREATE_FILE_A: u32 = 2;
    pub const CLOSE_HANDLE: u32 = 3;
    pub const READ_FILE: u32 = 4;
    pub const GET_FILE_ATTRIBUTES_A: u32 = 5;
    pub const GET_FILE_SIZE: u32 = 6;
    pub const FIND_FIRST_FILE_A: u32 = 7;
    pub const FIND_NEXT_FILE_A: u32 = 8;
    pub const FIND_CLOSE: u32 = 9;
    pub const CREATE_DIRECTORY_A: u32 = 10;
    pub const REMOVE_DIRECTORY_A: u32 = 11;
    pub const DELETE_FILE_A: u32 = 12;
    pub const MOVE_FILE_A: u32 = 13;
    pub const COPY_FILE_A: u32 = 14;
    pub const GET_CURRENT_DIRECTORY_A: u32 = 15;
    pub const SET_CURRENT_DIRECTORY_A: u32 = 16;
    // --- Process (Session 59) ---
    pub const CREATE_PIPE_A: u32 = 17;
    pub const CREATE_PROCESS_A: u32 = 18;
    pub const GET_EXIT_CODE_PROCESS: u32 = 19;
    pub const WAIT_FOR_SINGLE_OBJECT: u32 = 20;
    pub const TERMINATE_PROCESS: u32 = 21;
    pub const GET_CURRENT_PROCESS_ID: u32 = 22;
    pub const SET_HANDLE_INFORMATION: u32 = 23;
    // --- Time (Session 60) ---
    pub const GET_SYSTEM_TIME_AS_FILE_TIME: u32 = 24;
    pub const GET_TICK_COUNT_64: u32 = 25;
    pub const QUERY_PERFORMANCE_COUNTER: u32 = 26;
    pub const QUERY_PERFORMANCE_FREQUENCY: u32 = 27;
    // --- Environment (Session 65) ---
    pub const GET_ENVIRONMENT_VARIABLE_A: u32 = 28;
    pub const SET_ENVIRONMENT_VARIABLE_A: u32 = 29;
    pub const GET_ENVIRONMENT_STRINGS_A: u32 = 30;
    pub const FREE_ENVIRONMENT_STRINGS_A: u32 = 31;
    // --- Dynamic loading (Session 67) ---
    pub const LOAD_LIBRARY_A: u32 = 32;
    pub const GET_PROC_ADDRESS: u32 = 33;
}

/// The layout of the sections, computed from their contents.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) text_rva: u32,
    pub(crate) data_rva: u32,
    pub(crate) bss_rva: u32,
    pub(crate) idata_rva: u32,
    pub(crate) reloc_rva: u32,
    pub(crate) bss_size: u32,
    text_file_offset: u32,
    data_file_offset: u32,
    idata_file_offset: u32,
    reloc_file_offset: u32,
    size_of_image: u32,
}

/// Computes the virtual addresses and file offsets of the five sections.
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

struct Section {
    name: &'static [u8],
    contents: Vec<u8>,
    virtual_size: u32,
    raw_size: u32,
    file_offset: u32,
    characteristics: u32,
}

/// Builds the `.idata` section. Single DLL (kernel32.dll) only.
pub(crate) fn build_idata(idata_rva: u32) -> Vec<u8> {
    let k32_ilt_rva = idata_rva + ILT_OFFSET;
    let k32_iat_rva = idata_rva + IAT_OFFSET;

    let hint_name_start = idata_rva + ILT_OFFSET
        + (K32_COUNT + 1) * 8  // ILT entries + null
        + (K32_COUNT + 1) * 8; // IAT entries + null

    let mut hint_name_rvas = Vec::new();
    let mut offset = hint_name_start;
    for name in KERNEL32_IMPORTS {
        hint_name_rvas.push(offset);
        offset += 2 + name.len() as u32 + 1;
    }
    let dll_name_rva = offset;

    let mut bytes = Vec::with_capacity(IDATA_SIZE as usize);

    // Import descriptor
    bytes.extend_from_slice(&k32_ilt_rva.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    let name_pos = bytes.len();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // placeholder
    bytes.extend_from_slice(&k32_iat_rva.to_le_bytes());
    // Null descriptor
    bytes.extend_from_slice(&[0u8; 20]);

    // ILT entries
    for &rva in &hint_name_rvas {
        bytes.extend_from_slice(&(rva as u64).to_le_bytes());
    }
    bytes.extend_from_slice(&0u64.to_le_bytes());

    // IAT entries (same as ILT initially)
    for &rva in &hint_name_rvas {
        bytes.extend_from_slice(&(rva as u64).to_le_bytes());
    }
    bytes.extend_from_slice(&0u64.to_le_bytes());

    // Hint/Name entries
    for name in KERNEL32_IMPORTS {
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
    }

    // DLL name
    bytes.extend_from_slice(b"kernel32.dll\0");

    // Patch DLL name RVA
    bytes[name_pos..name_pos + 4].copy_from_slice(&dll_name_rva.to_le_bytes());

    bytes.resize(IDATA_SIZE as usize, 0);
    debug_assert_eq!(bytes.len(), IDATA_SIZE as usize);
    bytes
}

/// Builds a complete PE image.
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
        characteristics: 0x6000_0020,
    });
    if !data.is_empty() {
        sections.push(Section {
            name: b".data",
            contents: data.to_vec(),
            virtual_size: data.len() as u32,
            raw_size: align(data.len() as u32, FILE_ALIGNMENT),
            file_offset: layout.data_file_offset,
            characteristics: 0xC000_0040,
        });
    }
    sections.push(Section {
        name: b".bss",
        contents: Vec::new(),
        virtual_size: bss_size,
        raw_size: 0,
        file_offset: 0,
        characteristics: 0xC000_0080,
    });
    sections.push(Section {
        name: b".idata",
        contents: idata.to_vec(),
        virtual_size: idata.len() as u32,
        raw_size: align(idata.len() as u32, FILE_ALIGNMENT),
        file_offset: layout.idata_file_offset,
        characteristics: 0xC000_0040,
    });
    sections.push(Section {
        name: b".reloc",
        contents: reloc.to_vec(),
        virtual_size: reloc.len() as u32,
        raw_size: align(reloc.len() as u32, FILE_ALIGNMENT),
        file_offset: layout.reloc_file_offset,
        characteristics: 0x4200_0040,
    });

    let mut image = Vec::with_capacity(layout.size_of_image as usize);

    // DOS header
    image.extend_from_slice(b"MZ");
    image.resize(0x3C, 0);
    image.extend_from_slice(&0x80u32.to_le_bytes());
    image.resize(0x80, 0);

    // PE signature + COFF
    image.extend_from_slice(b"PE\0\0");
    image.extend_from_slice(&0x8664u16.to_le_bytes());
    image.extend_from_slice(&(sections.len() as u16).to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&240u16.to_le_bytes());
    image.extend_from_slice(&0x0022u16.to_le_bytes());

    // Optional header PE32+
    image.extend_from_slice(&0x20Bu16.to_le_bytes());
    image.push(0);
    image.push(0);
    image.extend_from_slice(&align(code.len() as u32, FILE_ALIGNMENT).to_le_bytes());
    let data_raw = if data.is_empty() {
        0
    } else {
        align(data.len() as u32, FILE_ALIGNMENT)
    };
    let idata_raw = align(idata.len() as u32, FILE_ALIGNMENT);
    let reloc_raw = align(reloc.len() as u32, FILE_ALIGNMENT);
    image.extend_from_slice(&(data_raw + idata_raw + reloc_raw).to_le_bytes());
    image.extend_from_slice(&align(bss_size, FILE_ALIGNMENT).to_le_bytes());
    image.extend_from_slice(&(layout.text_rva + entry_offset).to_le_bytes());
    image.extend_from_slice(&layout.text_rva.to_le_bytes());
    image.extend_from_slice(&IMAGE_BASE.to_le_bytes());
    image.extend_from_slice(&SECTION_ALIGNMENT.to_le_bytes());
    image.extend_from_slice(&FILE_ALIGNMENT.to_le_bytes());
    image.extend_from_slice(&6u16.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes());
    image.extend_from_slice(&6u16.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&layout.size_of_image.to_le_bytes());
    image.extend_from_slice(&SIZE_OF_HEADERS.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&3u16.to_le_bytes());
    image.extend_from_slice(&(0x0020u16 | 0x0100u16).to_le_bytes());
    image.extend_from_slice(&0x100000u64.to_le_bytes());
    image.extend_from_slice(&0x1000u64.to_le_bytes());
    image.extend_from_slice(&0x100000u64.to_le_bytes());
    image.extend_from_slice(&0x1000u64.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&16u32.to_le_bytes());
    for index in 0..16 {
        let (rva, size) = match index {
            1 => (layout.idata_rva, IMPORT_DIR_SIZE),
            5 => (layout.reloc_rva, reloc.len() as u32),
            _ => (0, 0),
        };
        image.extend_from_slice(&rva.to_le_bytes());
        image.extend_from_slice(&size.to_le_bytes());
    }

    // Section headers
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

    // Section data
    for section in &sections {
        if section.name == b".bss" {
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
    debug_assert!(name.len() <= 8);
    image.extend_from_slice(name);
    image.resize(image.len() + (8 - name.len()), 0);
    image.extend_from_slice(&virtual_size.to_le_bytes());
    image.extend_from_slice(&virtual_address.to_le_bytes());
    image.extend_from_slice(&raw_size.to_le_bytes());
    let pointer_to_raw_data = if name == b".bss" { 0 } else { file_offset };
    image.extend_from_slice(&pointer_to_raw_data.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes());
    image.extend_from_slice(&characteristics.to_le_bytes());
}

pub(crate) fn relocation_block(text_rva: u32) -> Vec<u8> {
    let mut block = Vec::with_capacity(12);
    block.extend_from_slice(&(text_rva & !0xFFF).to_le_bytes());
    block.extend_from_slice(&12u32.to_le_bytes());
    block.extend_from_slice(&0x0000u16.to_le_bytes());
    block.extend_from_slice(&0x0001u16.to_le_bytes());
    block
}
