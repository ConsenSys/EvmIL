// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use std::convert::TryFrom;
use std::error::Error;
use std::fs;

use clap::{arg, Arg, ArgMatches, Command};
use log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;
//
use evmil::evm::{AbstractStack, AbstractWord, Disassembly};
use evmil::il::Parser;
use evmil::ll::{Bytecode, Instruction};
use evmil::util::{w256, FromHexString, Interval, ToHexString};

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command-line arguments
    let matches = Command::new("evmil")
        .about("EvmIL Tool")
        .version("0.1.0")
        .subcommand_required(true)
        .arg(arg!(--verbose "Show verbose output"))
        .subcommand(
            Command::new("compile")
                .about("Compile EvmIL code to EVM bytecode")
                .arg(Arg::new("file").required(true))
                .visible_alias("c"),
        )
        .subcommand(
            Command::new("disassemble")
                .about("Disassemble a raw hex string into EVM bytecode")
                .arg(Arg::new("code").short('c').long("code"))
                .arg(Arg::new("target").required(true))
                .visible_alias("d"),
        )
        .get_matches();
    // Extract top-level flags
    let verbose = matches.is_present("verbose");
    // Initialise logging
    if verbose {
        init_logging(LevelFilter::Info);
    }
    // Dispatch on outcome
    let ok = match matches.subcommand() {
        Some(("compile", args)) => compile(args),
        Some(("disassemble", args)) => disassemble(args),
        _ => unreachable!(),
    }?;
    // Determine appropriate exit code
    let exitcode = if ok { 0 } else { 1 };
    // Done
    std::process::exit(exitcode);
}

/// Compile a given file.
fn compile(args: &ArgMatches) -> Result<bool, Box<dyn Error>> {
    // Extract the file to be compiled.
    let filename = args.get_one::<String>("file").unwrap();
    // Read the test file
    let input = fs::read_to_string(filename)?;
    // Parse test file
    let terms = Parser::new(&input).parse()?;
    // Translate statements into bytecode instructions
    let code = Bytecode::try_from(terms.as_slice()).unwrap();
    // Translate instructions into bytes
    let bytes: Vec<u8> = code.try_into().unwrap();
    // Print the final hex string
    println!("{}", bytes.to_hex_string());
    //
    Ok(true)
}

/// Disassemble a given bytecode sequence.
fn disassemble(args: &ArgMatches) -> Result<bool, Box<dyn Error>> {
    // Extract hex string to be disassembled.
    let mut hex = String::new();
    // Determine disassembly target
    let target = args.get_one::<String>("target").unwrap();
    // Decide whether bytecode provided directly, or via a file.
    if args.contains_id("code") {
        // Provided directly
        hex.push_str(target);
    } else {
        // Read hex from file
        let context = fs::read_to_string(target)?;
        // Read all lines of file
        for l in context.lines() { hex.push_str(l); }
    }
    // Parse hex string into bytes
    let bytes = hex.from_hex_string().unwrap();
    // Construct disassembly
    let disasm: Disassembly<AbstractStack<AbstractWord>> = Disassembly::new(&bytes).build();
    // Disassemble bytes into instructions
    let instructions = disasm.to_vec();
    // Print them all out.
    let mut pc = 0;
    for insn in instructions {
        match insn {
            Instruction::JUMPDEST(_) => {
                let st = disasm.get_state(pc);
                let len = st.stack.len();
                println!("");
                if len.is_constant() {
                    println!("// Stack +{}", len.unwrap());
                } else {
                    println!("// Stack +{}", len);
                }
                println!("{:#08x}: {}", pc, insn);
            }
            Instruction::JUMP | Instruction::JUMPI => {
                let st = disasm.get_state(pc);
                println!("{:#08x}: {} // {}", pc, insn, st.peek(0));
            }
            _ => {
                println!("{:#08x}: {}", pc, insn);
            }
        }
        pc = pc + insn.length(&[]); // broken
    }
    // TODO
    Ok(true)
}

/// Initialise logging using a suitable pattern.
pub fn init_logging(level: LevelFilter) {
    let encoder = PatternEncoder::new("[{l}] {m}{n}");
    //
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(encoder))
        .build();
    //
    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(level))
        .unwrap();
    //
    let _handle = log4rs::init_config(config).unwrap();
}
