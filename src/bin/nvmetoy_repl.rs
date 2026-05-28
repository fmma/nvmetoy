use std::env;
use std::fs::File;
use std::io::{self, BufRead};
use std::process::exit;

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::FileHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};

use nvmetoy::nvme_identify::print_controller_identification;
use nvmetoy::NvmeUserspaceDriver;

struct Extent {
    device_lba: u64,
    lba_count: u64,
    unwritten: bool,
}

struct FileBlocks {
    device_lba_size: u64,
    extents: Vec<Extent>,
}

impl FileBlocks {
    fn total_lbas(&self) -> u64 {
        self.extents.iter().map(|extent| extent.lba_count).sum()
    }

    fn check(&self, device_lba: u64, count: u64) -> io::Result<()> {
        if count == 0 {
            return Ok(());
        }
        let end = device_lba
            .checked_add(count)
            .ok_or_else(|| other("range overflows"))?;
        let mut covered = device_lba;
        while covered < end {
            let extent = self
                .extents
                .iter()
                .find(|extent| {
                    covered >= extent.device_lba && covered < extent.device_lba + extent.lba_count
                })
                .ok_or_else(|| other(&format!("block {covered} is not part of the file")))?;
            covered = extent.device_lba + extent.lba_count;
        }
        Ok(())
    }

    fn parse(reader: impl io::Read) -> io::Result<Self> {
        let mut device_lba_size = 0;
        let mut extents = Vec::new();
        for line in io::BufReader::new(reader).lines() {
            let line = line?;
            let fields: Vec<&str> = line.split_whitespace().collect();
            match fields.as_slice() {
                ["nvmetoy-block-map", "1"] => {}
                ["device_lba_size", value] => device_lba_size = parse(value)?,
                [device_lba, count, unwritten] => extents.push(Extent {
                    device_lba: parse(device_lba)?,
                    lba_count: parse(count)?,
                    unwritten: parse::<u8>(unwritten)? != 0,
                }),
                [] => {}
                _ => return Err(other(&format!("malformed block map line: {line}"))),
            }
        }
        Ok(FileBlocks {
            device_lba_size,
            extents,
        })
    }
}

const COMMANDS: &[&str] = &[
    "list", "identify", "read", "write", "fill", "peek", "poke", "help", "quit", "exit",
];

const LBA_COMMANDS: &[&str] = &["read", "write", "fill", "peek", "poke"];

struct ReplHelper {
    lbas: Vec<u64>,
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before = &line[..pos];
        let start = before.rfind(char::is_whitespace).map_or(0, |i| i + 1);
        let word = &before[start..];
        let prior: Vec<&str> = before[..start].split_whitespace().collect();

        let candidates = match prior.as_slice() {
            [] => COMMANDS
                .iter()
                .filter(|command| command.starts_with(word))
                .map(|command| Pair {
                    display: command.to_string(),
                    replacement: format!("{command} "),
                })
                .collect(),
            [command] if LBA_COMMANDS.contains(command) => self
                .lbas
                .iter()
                .filter(|lba| lba.to_string().starts_with(word))
                .map(|lba| Pair {
                    display: lba.to_string(),
                    replacement: format!("{lba} "),
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok((start, candidates))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;
}
impl Highlighter for ReplHelper {}
impl Validator for ReplHelper {}
impl Helper for ReplHelper {}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        exit(1);
    }
}

fn run() -> io::Result<()> {
    let arguments: Vec<String> = env::args().collect();
    let bdf = arg(&arguments, 1, "bdf")?;
    let blocks = load_blocks(arg(&arguments, 2, "map-file")?)?;
    let nsid: u32 = match arguments.get(3) {
        Some(value) => parse(value)?,
        None => 1,
    };
    let mut driver = open(bdf, nsid)?;

    println!(
        "nvmetoy repl: {} device blocks of {} bytes accessible (the file's blocks). \
         Type 'help', or Ctrl-D to quit.",
        blocks.total_lbas(),
        blocks.device_lba_size
    );

    let mut editor: Editor<ReplHelper, FileHistory> =
        Editor::new().map_err(|error| other(&format!("readline init failed: {error}")))?;
    let mut lbas: Vec<u64> = blocks.extents.iter().map(|extent| extent.device_lba).collect();
    lbas.sort_unstable();
    lbas.dedup();
    editor.set_helper(Some(ReplHelper { lbas }));
    let history = history_path();
    let _ = editor.load_history(&history);

    loop {
        match editor.readline("nvmetoy> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(line);
                match run_repl_line(&mut driver, &blocks, line) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => eprintln!("error: {error}"),
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(error) => return Err(other(&format!("readline error: {error}"))),
        }
    }

    let _ = editor.save_history(&history);
    Ok(())
}

fn run_repl_line(
    driver: &mut NvmeUserspaceDriver,
    blocks: &FileBlocks,
    line: &str,
) -> io::Result<bool> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    match tokens.as_slice() {
        ["help"] => repl_help(),
        ["quit"] | ["exit"] => return Ok(false),
        ["list"] => print_blocks(blocks),
        ["identify"] => print_controller_identification(&driver.identify_controller()?)?,
        ["read", lba, count] => {
            let device_lba = parse(lba)?;
            let data = confined_read(driver, blocks, device_lba, parse(count)?)?;
            print_read(device_lba, &data, blocks.device_lba_size as usize);
        }
        ["write", lba, rest @ ..] if !rest.is_empty() => {
            let device_lba = parse(lba)?;
            let text = rest.join(" ");
            let count = write_blocks(driver, blocks, device_lba, text.as_bytes())?;
            let plural = if count == 1 { "block" } else { "blocks" };
            println!(
                "wrote {} bytes into {count} {plural} at device-lba {device_lba} (block tail zeroed)",
                text.len()
            );
        }
        ["fill", lba, ch] => fill(driver, blocks, parse(lba)?, parse_ascii_char(ch)?, 1)?,
        ["fill", lba, ch, count] => fill(
            driver,
            blocks,
            parse(lba)?,
            parse_ascii_char(ch)?,
            parse(count)?,
        )?,
        ["peek", lba, offset, len] => {
            peek(driver, blocks, parse(lba)?, parse(offset)?, parse(len)?)?
        }
        ["poke", lba, offset, rest @ ..] if !rest.is_empty() => {
            poke(driver, blocks, parse(lba)?, parse(offset)?, &rest.join(" "))?
        }
        _ => return Err(other("unknown command; type 'help'")),
    }
    Ok(true)
}

fn repl_help() {
    println!(
        "commands (lba is an absolute device block, all text is ASCII, counts are decimal):\n  \
         list                     print the file's device blocks\n  \
         identify                 print the Identify Controller data\n  \
         read <lba> <count>       read count blocks, one block per line\n  \
         write <lba> <text>       overwrite whole block(s) with text, tail zeroed\n  \
         fill <lba> <char> [n]    write n blocks (default 1) of a repeated character\n  \
         peek <lba> <off> <n>     read n bytes at a byte offset within a block\n  \
         poke <lba> <off> <text>  patch a string at a byte offset, keeping the rest\n  \
         help                     this message\n  \
         quit | exit | Ctrl-D     leave the repl"
    );
}

fn confined_read(
    driver: &mut NvmeUserspaceDriver,
    blocks: &FileBlocks,
    device_lba: u64,
    count: u64,
) -> io::Result<Vec<u8>> {
    blocks.check(device_lba, count)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let block_count = u16::try_from(count).map_err(|_| other("read too large for one command"))?;
    driver.read(device_lba, block_count)
}

fn confined_write(
    driver: &mut NvmeUserspaceDriver,
    blocks: &FileBlocks,
    device_lba: u64,
    data: &[u8],
) -> io::Result<u64> {
    let block_size = blocks.device_lba_size as usize;
    if data.is_empty() || data.len() % block_size != 0 {
        return Err(other(&format!(
            "write payload must be a non-zero multiple of the {block_size}-byte block size"
        )));
    }
    let count = (data.len() / block_size) as u64;
    blocks.check(device_lba, count)?;
    driver.write(device_lba, data)?;
    Ok(count)
}

fn fill(
    driver: &mut NvmeUserspaceDriver,
    blocks: &FileBlocks,
    device_lba: u64,
    byte: u8,
    count: u64,
) -> io::Result<()> {
    if count == 0 {
        return Err(other("block count must be non-zero"));
    }
    let data = vec![byte; blocks.device_lba_size as usize * count as usize];
    let written = confined_write(driver, blocks, device_lba, &data)?;
    let plural = if written == 1 { "block" } else { "blocks" };
    println!(
        "wrote {written} {plural} ({} bytes) of '{}' at device-lba {device_lba}",
        data.len(),
        byte as char
    );
    Ok(())
}

fn peek(
    driver: &mut NvmeUserspaceDriver,
    blocks: &FileBlocks,
    device_lba: u64,
    offset: usize,
    len: usize,
) -> io::Result<()> {
    let block_size = blocks.device_lba_size as usize;
    if offset >= block_size || len > block_size - offset {
        return Err(other(&format!(
            "range at offset {offset} (len {len}) does not fit in the {block_size}-byte block"
        )));
    }
    let block = confined_read(driver, blocks, device_lba, 1)?;
    println!(
        "peek {len} bytes at device-lba {device_lba} offset {offset}: {}",
        ascii_line(&block[offset..offset + len])
    );
    Ok(())
}

fn write_blocks(
    driver: &mut NvmeUserspaceDriver,
    blocks: &FileBlocks,
    device_lba: u64,
    bytes: &[u8],
) -> io::Result<u64> {
    if bytes.is_empty() {
        return Err(other("nothing to write"));
    }
    let block_size = blocks.device_lba_size as usize;
    let mut buffer = vec![0u8; bytes.len().div_ceil(block_size) * block_size];
    buffer[..bytes.len()].copy_from_slice(bytes);
    confined_write(driver, blocks, device_lba, &buffer)
}

fn poke(
    driver: &mut NvmeUserspaceDriver,
    blocks: &FileBlocks,
    device_lba: u64,
    offset: usize,
    text: &str,
) -> io::Result<()> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err(other("nothing to write"));
    }
    let block_size = blocks.device_lba_size as usize;
    if offset >= block_size || bytes.len() > block_size - offset {
        return Err(other(&format!(
            "range at offset {offset} (len {}) does not fit in the {block_size}-byte block",
            bytes.len()
        )));
    }
    let mut buffer = confined_read(driver, blocks, device_lba, 1)?;
    buffer[offset..offset + bytes.len()].copy_from_slice(bytes);
    confined_write(driver, blocks, device_lba, &buffer)?;
    println!(
        "wrote {} bytes at device-lba {device_lba} offset {offset}",
        bytes.len()
    );
    Ok(())
}

fn print_blocks(blocks: &FileBlocks) {
    println!(
        "block size {} bytes, {} blocks total ({} bytes)",
        blocks.device_lba_size,
        blocks.total_lbas(),
        blocks.total_lbas() * blocks.device_lba_size
    );
    println!("{:>16}  {:>12}  state", "device-lba", "count");
    for extent in &blocks.extents {
        println!(
            "{:>16}  {:>12}  {}",
            extent.device_lba,
            extent.lba_count,
            if extent.unwritten {
                "unwritten"
            } else {
                "written"
            }
        );
    }
}

const READ_LINE_WIDTH: usize = 72;

fn print_read(device_lba: u64, data: &[u8], block_size: usize) {
    println!("read {} bytes from device-lba {device_lba}:", data.len());
    // For long reads, show only the first HEAD and last TAIL block-lines so the output
    // stays readable; the middle is summarized as a single elided line.
    const HEAD: usize = 10;
    const TAIL: usize = 10;
    let total = data.len().div_ceil(block_size);
    for (index, block) in data.chunks(block_size).enumerate() {
        if total > HEAD + TAIL && index >= HEAD && index < total - TAIL {
            if index == HEAD {
                println!("         ... {} blocks omitted ...", total - HEAD - TAIL);
            }
            continue;
        }
        print_read_line(device_lba + index as u64, block);
    }
}

fn print_read_line(lba: u64, block: &[u8]) {
    if block.len() > READ_LINE_WIDTH {
        let hidden = block.len() - READ_LINE_WIDTH;
        println!(
            "{lba:>10}: {}  [+{hidden} bytes]",
            ascii_line(&block[..READ_LINE_WIDTH])
        );
    } else {
        println!("{lba:>10}: {}", ascii_line(block));
    }
}

fn open(bdf: &str, nsid: u32) -> io::Result<NvmeUserspaceDriver> {
    NvmeUserspaceDriver::open(bdf, nsid).map_err(|error| {
        other(&format!(
            "failed to open controller {bdf}: {error} \
             (run as root, with the device unbound from the kernel nvme driver)"
        ))
    })
}

fn load_blocks(path: &str) -> io::Result<FileBlocks> {
    FileBlocks::parse(File::open(path)?)
}

fn history_path() -> String {
    match env::var("HOME") {
        Ok(home) => format!("{home}/.nvmetoy_history"),
        Err(_) => "/tmp/nvmetoy_history".to_string(),
    }
}

fn parse_ascii_char(text: &str) -> io::Result<u8> {
    match text.as_bytes() {
        [byte] => Ok(*byte),
        _ => Err(other(&format!(
            "fill character must be a single ASCII character: {text:?}"
        ))),
    }
}

const NON_PRINTABLE: char = '·'; // U+00B7 middle dot

fn ascii_char(byte: u8) -> char {
    match byte {
        0x20..=0x7e => byte as char,
        0x00..=0x1f => char::from_u32(0x2400 + byte as u32).unwrap_or(NON_PRINTABLE),
        0x7f => '␡', // U+2421 symbol for delete
        _ => NON_PRINTABLE,
    }
}

fn ascii_line(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| ascii_char(byte)).collect()
}

fn arg<'a>(arguments: &'a [String], index: usize, name: &str) -> io::Result<&'a str> {
    arguments
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| other(&format!("missing argument: {name}")))
}

fn parse<T: std::str::FromStr>(text: &str) -> io::Result<T> {
    text.parse()
        .map_err(|_| other(&format!("cannot parse {text:?}")))
}

fn other(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, message.to_string())
}
