use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const YEX_MAGIC: &[u8; 4] = b"YEXE";
const YEX_HEADER_LEN: usize = 8;
const MAX_YEX_FILE_LEN: usize = 64 * 1024;
const ELF64_HEADER_LEN: usize = 64;
const ELF64_PROGRAM_HEADER_LEN: usize = 56;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;

#[derive(Debug, PartialEq, Eq)]
enum ConvertError {
    Truncated(&'static str),
    NotElf,
    UnsupportedClass,
    UnsupportedEndian,
    UnsupportedType(u16),
    UnsupportedMachine(u16),
    InvalidProgramHeaderSize,
    InvalidProgramHeaderTable,
    UnsupportedLoadSegmentCount(usize),
    EntryNotInExecutableSegment,
    LoadSegmentHasBss,
    EmptyPayload,
    PayloadTooLarge,
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(part) => write!(f, "ELF is truncated ({part})"),
            Self::NotElf => write!(f, "input is not an ELF file"),
            Self::UnsupportedClass => write!(f, "only ELF64 is supported"),
            Self::UnsupportedEndian => write!(f, "only little-endian ELF is supported"),
            Self::UnsupportedType(kind) => {
                write!(f, "unsupported ELF type {kind}; expected ET_EXEC")
            }
            Self::UnsupportedMachine(machine) => {
                write!(f, "unsupported ELF machine {machine}; expected x86-64")
            }
            Self::InvalidProgramHeaderSize => {
                write!(
                    f,
                    "ELF program-header entries are smaller than ELF64 headers"
                )
            }
            Self::InvalidProgramHeaderTable => write!(f, "invalid ELF program-header table"),
            Self::UnsupportedLoadSegmentCount(count) => {
                write!(f, "expected exactly one PT_LOAD segment, found {count}")
            }
            Self::EntryNotInExecutableSegment => {
                write!(
                    f,
                    "ELF entry point is not inside the executable PT_LOAD segment"
                )
            }
            Self::LoadSegmentHasBss => {
                write!(f, "PT_LOAD has uninitialized memory; BSS is not supported")
            }
            Self::EmptyPayload => write!(f, "ELF has no executable bytes at its entry point"),
            Self::PayloadTooLarge => write!(f, "YEXE exceeds the kernel's 64 KiB image limit"),
        }
    }
}

impl Error for ConvertError {}

#[derive(Debug)]
struct LoadSegment {
    flags: u32,
    file_offset: usize,
    virtual_address: u64,
    file_size: usize,
    memory_size: u64,
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ConvertError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(ConvertError::Truncated("ELF header"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ConvertError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(ConvertError::Truncated("program header"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ConvertError> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(ConvertError::Truncated("ELF header"))?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

fn convert_elf_to_yex(elf: &[u8]) -> Result<Vec<u8>, ConvertError> {
    if elf.get(..4) != Some(b"\x7FELF") {
        return Err(ConvertError::NotElf);
    }
    if elf.get(4) != Some(&2) {
        return Err(ConvertError::UnsupportedClass);
    }
    if elf.get(5) != Some(&1) {
        return Err(ConvertError::UnsupportedEndian);
    }
    if elf.len() < ELF64_HEADER_LEN {
        return Err(ConvertError::Truncated("ELF64 header"));
    }

    let elf_type = read_u16(elf, 16)?;
    if elf_type != ET_EXEC {
        return Err(ConvertError::UnsupportedType(elf_type));
    }
    let machine = read_u16(elf, 18)?;
    if machine != EM_X86_64 {
        return Err(ConvertError::UnsupportedMachine(machine));
    }

    let entry = read_u64(elf, 24)?;
    let program_header_offset =
        usize::try_from(read_u64(elf, 32)?).map_err(|_| ConvertError::InvalidProgramHeaderTable)?;
    let program_header_size = read_u16(elf, 54)? as usize;
    let program_header_count = read_u16(elf, 56)? as usize;
    if program_header_size < ELF64_PROGRAM_HEADER_LEN {
        return Err(ConvertError::InvalidProgramHeaderSize);
    }
    let table_size = program_header_size
        .checked_mul(program_header_count)
        .and_then(|size| program_header_offset.checked_add(size))
        .ok_or(ConvertError::InvalidProgramHeaderTable)?;
    if table_size > elf.len() {
        return Err(ConvertError::InvalidProgramHeaderTable);
    }

    let mut load_segments = Vec::new();
    for index in 0..program_header_count {
        let offset = program_header_offset + index * program_header_size;
        if read_u32(elf, offset)? != PT_LOAD {
            continue;
        }
        let file_offset = usize::try_from(read_u64(elf, offset + 8)?)
            .map_err(|_| ConvertError::InvalidProgramHeaderTable)?;
        let file_size = usize::try_from(read_u64(elf, offset + 32)?)
            .map_err(|_| ConvertError::InvalidProgramHeaderTable)?;
        load_segments.push(LoadSegment {
            flags: read_u32(elf, offset + 4)?,
            file_offset,
            virtual_address: read_u64(elf, offset + 16)?,
            file_size,
            memory_size: read_u64(elf, offset + 40)?,
        });
    }

    if load_segments.len() != 1 {
        return Err(ConvertError::UnsupportedLoadSegmentCount(
            load_segments.len(),
        ));
    }
    let segment = &load_segments[0];
    if segment.flags & PF_X == 0 || segment.memory_size != segment.file_size as u64 {
        return Err(if segment.memory_size != segment.file_size as u64 {
            ConvertError::LoadSegmentHasBss
        } else {
            ConvertError::EntryNotInExecutableSegment
        });
    }

    let segment_end = segment
        .virtual_address
        .checked_add(segment.file_size as u64)
        .ok_or(ConvertError::EntryNotInExecutableSegment)?;
    if entry < segment.virtual_address || entry >= segment_end {
        return Err(ConvertError::EntryNotInExecutableSegment);
    }

    let entry_offset = usize::try_from(entry - segment.virtual_address)
        .map_err(|_| ConvertError::EntryNotInExecutableSegment)?;
    let payload_start = segment
        .file_offset
        .checked_add(entry_offset)
        .ok_or(ConvertError::InvalidProgramHeaderTable)?;
    let payload_end = segment
        .file_offset
        .checked_add(segment.file_size)
        .ok_or(ConvertError::InvalidProgramHeaderTable)?;
    let payload = elf
        .get(payload_start..payload_end)
        .ok_or(ConvertError::InvalidProgramHeaderTable)?;
    if payload.is_empty() {
        return Err(ConvertError::EmptyPayload);
    }
    if payload.len() > MAX_YEX_FILE_LEN - YEX_HEADER_LEN {
        return Err(ConvertError::PayloadTooLarge);
    }
    let payload_len = u32::try_from(payload.len()).map_err(|_| ConvertError::PayloadTooLarge)?;

    let mut yex = Vec::with_capacity(YEX_HEADER_LEN + payload.len());
    yex.extend_from_slice(YEX_MAGIC);
    yex.extend_from_slice(&payload_len.to_le_bytes());
    yex.extend_from_slice(payload);
    Ok(yex)
}

fn output_path(input: &Path) -> PathBuf {
    input.with_extension("yex")
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.is_empty() || args.len() > 2 {
        return Err("usage: elf2yex <input.elf> [output.yex]".into());
    }

    let input = PathBuf::from(&args[0]);
    let output = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| output_path(&input));
    let elf = fs::read(&input)?;
    let yex = convert_elf_to_yex(&elf)?;
    fs::write(&output, &yex)?;
    println!(
        "converted {} -> {} ({} payload bytes)",
        input.display(),
        output.display(),
        yex.len() - YEX_HEADER_LEN
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("elf2yex: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_elf(entry_offset: usize, payload: &[u8]) -> Vec<u8> {
        let program_header_offset = ELF64_HEADER_LEN;
        let payload_offset = program_header_offset + ELF64_PROGRAM_HEADER_LEN;
        let mut elf = vec![0; payload_offset + payload.len()];
        elf[0..4].copy_from_slice(b"\x7FELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[6] = 1;
        elf[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        elf[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        let virtual_address = 0x400000u64;
        elf[24..32].copy_from_slice(&(virtual_address + entry_offset as u64).to_le_bytes());
        elf[32..40].copy_from_slice(&(program_header_offset as u64).to_le_bytes());
        elf[52..54].copy_from_slice(&(ELF64_HEADER_LEN as u16).to_le_bytes());
        elf[54..56].copy_from_slice(&(ELF64_PROGRAM_HEADER_LEN as u16).to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());

        elf[program_header_offset..program_header_offset + 4]
            .copy_from_slice(&PT_LOAD.to_le_bytes());
        elf[program_header_offset + 4..program_header_offset + 8]
            .copy_from_slice(&PF_X.to_le_bytes());
        elf[program_header_offset + 8..program_header_offset + 16]
            .copy_from_slice(&(payload_offset as u64).to_le_bytes());
        elf[program_header_offset + 16..program_header_offset + 24]
            .copy_from_slice(&virtual_address.to_le_bytes());
        elf[program_header_offset + 32..program_header_offset + 40]
            .copy_from_slice(&(payload.len() as u64).to_le_bytes());
        elf[program_header_offset + 40..program_header_offset + 48]
            .copy_from_slice(&(payload.len() as u64).to_le_bytes());
        elf[payload_offset..].copy_from_slice(payload);
        elf
    }

    #[test]
    fn wraps_executable_bytes_from_entry_point() {
        let elf = minimal_elf(2, &[0x90, 0x90, 0xC3]);
        let yex = convert_elf_to_yex(&elf).unwrap();
        assert_eq!(&yex[..4], YEX_MAGIC);
        assert_eq!(u32::from_le_bytes(yex[4..8].try_into().unwrap()), 1);
        assert_eq!(&yex[8..], &[0xC3]);
    }

    #[test]
    fn rejects_non_elf_input() {
        assert_eq!(convert_elf_to_yex(b"not elf"), Err(ConvertError::NotElf));
    }

    #[test]
    fn rejects_elf_with_bss() {
        let mut elf = minimal_elf(0, &[0xC3]);
        let memory_size_offset = ELF64_HEADER_LEN + 40;
        elf[memory_size_offset..memory_size_offset + 8].copy_from_slice(&2u64.to_le_bytes());
        assert_eq!(
            convert_elf_to_yex(&elf),
            Err(ConvertError::LoadSegmentHasBss)
        );
    }
}
