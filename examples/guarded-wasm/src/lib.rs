//! Single-threaded buffer interface for the compiled grammar prototype.
use oracle_ir_grammar::engine::{Engine, canonical_abilities};
use oracle_ir_grammar::model::ir::SCHEMA_VERSION;
use serde::Deserialize;
use serde_json::{Value, json};
use std::cell::RefCell;

const TABLE: &[u8] = include_bytes!(env!("ORACLE_SLICE_PATH"));
const MAX_REQUEST: u32 = 1024 * 1024;

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

#[derive(Deserialize)]
struct Request {
    name: String,
    #[serde(default)]
    types: Vec<String>,
    #[serde(flatten)]
    command: Command,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Command {
    Parse { text: String },
    Print { reading: Value },
    Verify { text: String, reading: Value },
}

#[unsafe(no_mangle)]
pub extern "C" fn input(len: u32) -> *mut u8 {
    INPUT.with_borrow_mut(|buffer| {
        if len > MAX_REQUEST {
            buffer.clear();
            return std::ptr::null_mut();
        }
        buffer.resize(len as usize, 0);
        buffer.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn output() -> *const u8 {
    OUTPUT.with_borrow(|buffer| buffer.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn init() -> usize {
    response(ENGINE.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(Engine::from_compiled(TABLE)?);
        }
        let rules = slot.as_ref().map_or(0, Engine::rule_count);
        Ok(json!({"rules": rules, "schema": SCHEMA_VERSION}))
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn run() -> usize {
    response(ENGINE.with_borrow(|slot| {
        let engine = slot.as_ref().ok_or("initialize the grammar first")?;
        let request: Request = INPUT
            .with_borrow(|buffer| serde_json::from_slice(buffer))
            .map_err(|error| error.to_string())?;
        let parse = |text: &str| {
            engine
                .parse_types(&request.name, text, &request.types)
                .map_err(|error| format!("{error:?}"))
        };
        match request.command {
            Command::Parse { text } => Ok(json!({"readings": parse(&text)?.readings})),
            Command::Print { reading } => {
                let text = engine.render(&request.name, &reading, &request.types, 1)?;
                let text = text.into_iter().next();
                Ok(json!({"text": text}))
            }
            Command::Verify { text, reading } => {
                let wanted = canonical_abilities(&reading)?;
                Ok(json!({"verified": parse(&text)?.readings.contains(&wanted)}))
            }
        }
    }))
}

fn response(result: Result<Value, String>) -> usize {
    let value = result.unwrap_or_else(|error| json!({"error": error}));
    OUTPUT.with_borrow_mut(|buffer| {
        buffer.clear();
        if serde_json::to_writer(&mut *buffer, &value).is_err() {
            buffer.clear();
            buffer.extend_from_slice(br#"{"error":"response encoding"}"#);
        }
        buffer.len()
    })
}
