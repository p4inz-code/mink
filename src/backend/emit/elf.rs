//! ELF64 executable image format for Linux x86_64.
//!
//! Generates a static ELF executable that uses raw Linux syscalls for the
//! minimal runtime (exit, write). This avoids PLT/GOT/dynamic linking
//! complexity for V1.

/// ELF constants
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LSB: u8 = 1;
const ELF_VERSION_1: u8 = 1;
const ELF_OSABI_NONE: u8 = 0;
const ELF_TYPE_EXEC: u16 = 2;
const ELF_MACHINE_X86_64: u16 = 0x3E;
const ELF_ENTRY_SIZE_64: u16 = 64;
const ELF_PHDR_ENTRY_SIZE: u16 = 56;
const PT_LOAD: u32 = 1;
const PT_GNU_STACK: u32 = 6;
const PF_R: u32 = 4;
const PF_W: u32 = 2;
const PF_X: u32 = 1;

/// The base virtual address for the executable.
const BASE_ADDR: u64 = 0x400000;

/// The entry point offset within .text (after ELF/phdr headers).
const ENTRY_OFFSET: u64 = 0x1000;

/// The alignment for LOAD segments.
const PAGE_SIZE: u64 = 0x1000;

/// Build an ELF64 executable image.
pub(crate) fn build_elf(text: &[u8], data: &[u8], bss_size: u32, entry_offset: u32) -> Vec<u8> {
    // Layout:
    //   [0x0000..0x0FFF]  ELF header + program headers (1 page)
    //   [0x1000..]        .text (code)
    //   [after .text]     .rodata (string data, merged into text segment)
    //   [next page]       .data (initialized data)
    //   [after .data]     .bss (zero-filled)

    let text_addr = BASE_ADDR + ENTRY_OFFSET;
    let text_file_size = text.len() as u64;
    let text_mem_size = text_file_size;

    // Data segment starts at next page boundary after text
    let data_file_offset = align_up(ENTRY_OFFSET + text_file_size, PAGE_SIZE);
    let data_addr = BASE_ADDR + data_file_offset;
    let data_file_size = data.len() as u64;
    let data_mem_size = data_file_size + bss_size as u64;

    let total_size = (data_file_offset + data_file_size) as usize;
    let mut image = vec![0u8; total_size];

    // --- ELF header (64 bytes) ---
    image[0..4].copy_from_slice(&ELF_MAGIC);
    image[4] = ELF_CLASS_64;
    image[5] = ELF_DATA_LSB;
    image[6] = ELF_VERSION_1;
    image[7] = ELF_OSABI_NONE;
    // e_ident[8..16] = padding (zero-initialized)
    // e_type at offset 16 = ET_EXEC
    image[16..18].copy_from_slice(&ELF_TYPE_EXEC.to_le_bytes());
    // e_machine at offset 18 = EM_X86_64
    image[18..20].copy_from_slice(&ELF_MACHINE_X86_64.to_le_bytes());
    // e_version at offset 20 = EV_CURRENT (1)
    image[20..24].copy_from_slice(&1u32.to_le_bytes());
    // e_entry at offset 24 = BASE_ADDR + entry_offset (in .text)
    let entry_addr = text_addr + entry_offset as u64;
    image[24..32].copy_from_slice(&entry_addr.to_le_bytes());
    // e_phoff at offset 32 = 64 (right after ELF header)
    image[32..40].copy_from_slice(&64u64.to_le_bytes());
    // e_shoff at offset 40 = 0 (no section headers needed for execution)
    image[40..48].copy_from_slice(&0u64.to_le_bytes());
    // e_flags at offset 48 = 0
    image[48..52].copy_from_slice(&0u32.to_le_bytes());
    // e_ehsize at offset 52 = 64
    image[52..54].copy_from_slice(&64u16.to_le_bytes());
    // e_phentsize at offset 54 = 56
    image[54..56].copy_from_slice(&ELF_PHDR_ENTRY_SIZE.to_le_bytes());
    // e_phnum at offset 56 = 3 (text, data, stack)
    image[56..58].copy_from_slice(&3u16.to_le_bytes());
    // e_shentsize at offset 58 = 0
    image[58..60].copy_from_slice(&0u16.to_le_bytes());
    // e_shnum at offset 60 = 0
    image[60..62].copy_from_slice(&0u16.to_le_bytes());
    // e_shstrndx at offset 62 = 0
    image[62..64].copy_from_slice(&0u16.to_le_bytes());

    // --- Program header 1: .text + .rodata (RX) ---
    let phdr1_offset = 64usize;
    write_phdr(
        &mut image[phdr1_offset..phdr1_offset + 56],
        PT_LOAD,
        PF_R | PF_X,
        ENTRY_OFFSET,
        text_addr,
        text_file_size,
        text_mem_size,
        PAGE_SIZE,
    );

    // --- Program header 2: .data + .bss (RW) ---
    let phdr2_offset = phdr1_offset + 56;
    write_phdr(
        &mut image[phdr2_offset..phdr2_offset + 56],
        PT_LOAD,
        PF_R | PF_W,
        data_file_offset,
        data_addr,
        data_file_size,
        data_mem_size,
        PAGE_SIZE,
    );

    // --- Program header 3: GNU_STACK (RW, no exec) ---
    let phdr3_offset = phdr2_offset + 56;
    write_phdr(
        &mut image[phdr3_offset..phdr3_offset + 56],
        PT_GNU_STACK,
        PF_R | PF_W,
        0, // no file backing
        0,
        0,
        0,
        16, // alignment
    );

    // --- Copy .text ---
    image[ENTRY_OFFSET as usize..ENTRY_OFFSET as usize + text.len()].copy_from_slice(text);

    // --- Copy .data ---
    let data_dest = data_file_offset as usize;
    image[data_dest..data_dest + data.len()].copy_from_slice(data);

    image
}

/// Write a 64-bit ELF program header entry.
fn write_phdr(
    buf: &mut [u8],
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
) {
    buf[0..4].copy_from_slice(&p_type.to_le_bytes());
    buf[4..8].copy_from_slice(&p_flags.to_le_bytes());
    buf[8..16].copy_from_slice(&p_offset.to_le_bytes());
    buf[16..24].copy_from_slice(&p_vaddr.to_le_bytes());
    buf[24..32].copy_from_slice(&p_vaddr.to_le_bytes()); // p_paddr = p_vaddr
    buf[32..40].copy_from_slice(&p_filesz.to_le_bytes());
    buf[40..48].copy_from_slice(&p_memsz.to_le_bytes());
    buf[48..56].copy_from_slice(&p_align.to_le_bytes());
}

/// Round up to the given alignment (must be a power of 2).
fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}
