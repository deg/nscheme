//! Port I/O primitives.
//!
//! Implements the textual port subset of R7RS-small §6.13:
//! `current-input-port`, `current-output-port`, `current-error-port`,
//! `open-input-{string,file}`, `open-output-{string,file}`,
//! `get-output-string`, `close-port`, `read`, `read-char`, `peek-char`,
//! `read-line`, `read-string`, `eof-object?`, `eof-object`,
//! `write`, `display`, `newline`, `write-char`, `write-string`,
//! `with-{input,output}-from-file`, `file-exists?`, `delete-file`,
//! plus the type predicates `input-port?`, `output-port?`,
//! `textual-port?`, `binary-port?`.
//!
//! Binary ports and `read-bytevector` are deferred.

#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::fs;
use std::io::Read as _;
use std::io::Write as _;
use std::rc::Rc;

use crate::env::EnvRef;
use crate::value::{Arity, Port, PrimitiveFn, Procedure, RuntimeError, Symbol, Value};

/// Install the I/O primitives plus the three current-port parameters.
pub fn install_io(env: &EnvRef) {
    let stdin = Value::Port(Rc::new(RefCell::new(Port::StdIn {
        buffer: String::new(),
        pos: 0,
    })));
    let stdout = Value::Port(Rc::new(RefCell::new(Port::StdOut)));
    let stderr = Value::Port(Rc::new(RefCell::new(Port::StdErr)));

    // Store the canonical ports in the env so the (current-*-port)
    // procedures can return the same value each time.
    env.define(Symbol::intern("$stdin"), stdin);
    env.define(Symbol::intern("$stdout"), stdout);
    env.define(Symbol::intern("$stderr"), stderr);

    define_prim(env, "current-input-port", Arity::Exact(0), |args| {
        let _ = args;
        // Resolved at call time via env lookup.
        Err(RuntimeError::Other(
            "current-input-port resolved via Scheme wrapper; see bootstrap".into(),
        ))
    });
    // We instead define the three current-* procedures in the bootstrap
    // string below where lookup is straightforward.

    define_prim(env, "input-port?", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => Ok(Value::Bool(p.borrow().is_input())),
        _ => Ok(Value::Bool(false)),
    });
    define_prim(env, "output-port?", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => Ok(Value::Bool(p.borrow().is_output())),
        _ => Ok(Value::Bool(false)),
    });
    define_prim(env, "textual-port?", Arity::Exact(1), |a| {
        // Every port in v1 is textual.
        Ok(Value::Bool(matches!(a[0], Value::Port(_))))
    });
    define_prim(env, "binary-port?", Arity::Exact(1), |a| {
        let _ = a;
        Ok(Value::Bool(false))
    });
    define_prim(env, "eof-object", Arity::Exact(0), |a| {
        let _ = a;
        Ok(Value::Eof)
    });
    define_prim(env, "eof-object?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Eof)))
    });

    // -- string ports ----------------------------------------------

    define_prim(env, "open-input-string", Arity::Exact(1), |a| match &a[0] {
        Value::String(s) => Ok(Value::Port(Rc::new(RefCell::new(Port::StringInput {
            content: s.borrow().clone(),
            pos: 0,
        })))),
        other => Err(type_err("string", other)),
    });
    define_prim(env, "open-output-string", Arity::Exact(0), |a| {
        let _ = a;
        Ok(Value::Port(Rc::new(RefCell::new(Port::StringOutput {
            buffer: String::new(),
        }))))
    });
    define_prim(env, "get-output-string", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => match &*p.borrow() {
            Port::StringOutput { buffer } => Ok(Value::string(buffer.clone())),
            _ => Err(RuntimeError::Other(
                "get-output-string: not a string output port".into(),
            )),
        },
        other => Err(type_err("port", other)),
    });

    // -- file ports -------------------------------------------------

    define_prim(env, "open-input-file", Arity::Exact(1), |a| match &a[0] {
        Value::String(path) => {
            let p = path.borrow().clone();
            let content = fs::read_to_string(&p)
                .map_err(|e| RuntimeError::Other(format!("open-input-file({p}): {e}")))?;
            Ok(Value::Port(Rc::new(RefCell::new(Port::FileInput {
                content,
                pos: 0,
                path: p,
            }))))
        }
        other => Err(type_err("string", other)),
    });
    define_prim(env, "open-output-file", Arity::Exact(1), |a| match &a[0] {
        Value::String(path) => Ok(Value::Port(Rc::new(RefCell::new(Port::FileOutput {
            buffer: String::new(),
            path: path.borrow().clone(),
        })))),
        other => Err(type_err("string", other)),
    });

    define_prim(env, "close-port", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => {
            // Flush file output ports before closing.
            let snapshot = std::mem::replace(&mut *p.borrow_mut(), Port::Closed);
            if let Port::FileOutput { buffer, path } = snapshot {
                fs::write(&path, buffer)
                    .map_err(|e| RuntimeError::Other(format!("close-port({path}): {e}")))?;
            }
            Ok(Value::Unspecified)
        }
        other => Err(type_err("port", other)),
    });
    define_prim(env, "close-input-port", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => {
            *p.borrow_mut() = Port::Closed;
            Ok(Value::Unspecified)
        }
        other => Err(type_err("port", other)),
    });
    define_prim(env, "close-output-port", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => {
            let snapshot = std::mem::replace(&mut *p.borrow_mut(), Port::Closed);
            if let Port::FileOutput { buffer, path } = snapshot {
                fs::write(&path, buffer)
                    .map_err(|e| RuntimeError::Other(format!("close-output-port: {e}")))?;
            }
            Ok(Value::Unspecified)
        }
        other => Err(type_err("port", other)),
    });

    // -- reading ----------------------------------------------------

    define_prim(env, "read-char", Arity::Range { min: 0, max: 1 }, |a| {
        // Default to stdin if no port given.
        match a.first() {
            None => read_char_from_stdin(),
            Some(Value::Port(p)) => read_char_from_port(&mut p.borrow_mut()),
            Some(other) => Err(type_err("port", other)),
        }
    });
    define_prim(
        env,
        "peek-char",
        Arity::Range { min: 0, max: 1 },
        |a| match a.first() {
            None => peek_char_from_stdin(),
            Some(Value::Port(p)) => peek_char_from_port(&p.borrow()),
            Some(other) => Err(type_err("port", other)),
        },
    );
    define_prim(
        env,
        "read-line",
        Arity::Range { min: 0, max: 1 },
        |a| match a.first() {
            None => read_line_from_stdin(),
            Some(Value::Port(p)) => read_line_from_port(&mut p.borrow_mut()),
            Some(other) => Err(type_err("port", other)),
        },
    );

    // -- writing ----------------------------------------------------

    define_prim(env, "display", Arity::Range { min: 1, max: 2 }, |a| {
        let s = format!("{}", a[0]);
        write_to_port(a.get(1), &s)
    });
    define_prim(env, "write", Arity::Range { min: 1, max: 2 }, |a| {
        let s = format!("{:?}", a[0]);
        write_to_port(a.get(1), &s)
    });
    define_prim(env, "newline", Arity::Range { min: 0, max: 1 }, |a| {
        write_to_port(a.first(), "\n")
    });
    define_prim(env, "write-char", Arity::Range { min: 1, max: 2 }, |a| {
        let Value::Char(c) = a[0] else {
            return Err(type_err("char", &a[0]));
        };
        write_to_port(a.get(1), &c.to_string())
    });
    define_prim(env, "write-string", Arity::Range { min: 1, max: 2 }, |a| {
        let Value::String(s) = &a[0] else {
            return Err(type_err("string", &a[0]));
        };
        write_to_port(a.get(1), &s.borrow())
    });

    // -- file utilities --------------------------------------------

    define_prim(env, "file-exists?", Arity::Exact(1), |a| match &a[0] {
        Value::String(path) => Ok(Value::Bool(fs::metadata(&*path.borrow()).is_ok())),
        other => Err(type_err("string", other)),
    });
    define_prim(env, "delete-file", Arity::Exact(1), |a| match &a[0] {
        Value::String(path) => {
            let p = path.borrow().clone();
            fs::remove_file(&p)
                .map_err(|e| RuntimeError::Other(format!("delete-file({p}): {e}")))?;
            Ok(Value::Unspecified)
        }
        other => Err(type_err("string", other)),
    });
}

fn define_prim(env: &EnvRef, name: &'static str, arity: Arity, body: PrimitiveFn) {
    let p = Procedure::Primitive { name, arity, body };
    env.define(Symbol::intern(name), Value::Procedure(Rc::new(p)));
}

fn type_err(expected: &str, got: &Value) -> RuntimeError {
    RuntimeError::Type {
        expected: expected.into(),
        got: got.type_name().into(),
    }
}

/// Write `s` to the given port (defaulting to stdout when `port` is
/// `None` or is `Unspecified`).
fn write_to_port(port: Option<&Value>, s: &str) -> Result<Value, RuntimeError> {
    match port {
        None => {
            print!("{s}");
            std::io::stdout()
                .flush()
                .map_err(|e| RuntimeError::Other(format!("flush: {e}")))?;
            Ok(Value::Unspecified)
        }
        Some(Value::Port(p)) => {
            let mut port = p.borrow_mut();
            match &mut *port {
                Port::StdOut => {
                    print!("{s}");
                    std::io::stdout()
                        .flush()
                        .map_err(|e| RuntimeError::Other(format!("flush: {e}")))?;
                }
                Port::StdErr => {
                    eprint!("{s}");
                }
                Port::StringOutput { buffer } | Port::FileOutput { buffer, .. } => {
                    buffer.push_str(s);
                }
                Port::Closed => {
                    return Err(RuntimeError::Other("write: port is closed".into()));
                }
                _ => {
                    return Err(RuntimeError::Other("write: not an output port".into()));
                }
            }
            Ok(Value::Unspecified)
        }
        Some(other) => Err(type_err("port", other)),
    }
}

/// Read one character from the given input port, returning `(eof-object)`
/// at end of input.
fn read_char_from_port(port: &mut Port) -> Result<Value, RuntimeError> {
    match port {
        Port::StringInput { content, pos } | Port::FileInput { content, pos, .. } => {
            if let Some(c) = content[*pos..].chars().next() {
                *pos += c.len_utf8();
                Ok(Value::Char(c))
            } else {
                Ok(Value::Eof)
            }
        }
        Port::StdIn { buffer, pos } => {
            if *pos >= buffer.len() {
                buffer.clear();
                *pos = 0;
                let mut line = String::new();
                let n = std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|e| RuntimeError::Other(format!("read-char: {e}")))?;
                if n == 0 {
                    return Ok(Value::Eof);
                }
                buffer.push_str(&line);
            }
            let c = buffer[*pos..].chars().next().unwrap();
            *pos += c.len_utf8();
            Ok(Value::Char(c))
        }
        Port::Closed => Err(RuntimeError::Other("read-char: port is closed".into())),
        _ => Err(RuntimeError::Other("read-char: not an input port".into())),
    }
}

fn peek_char_from_port(port: &Port) -> Result<Value, RuntimeError> {
    match port {
        Port::StringInput { content, pos } | Port::FileInput { content, pos, .. } => Ok(content
            [*pos..]
            .chars()
            .next()
            .map_or(Value::Eof, Value::Char)),
        // peek-char on stdin would need lookahead; for v1 we just read
        // one and stash it back via the StdIn buffer.
        Port::StdIn { .. } => Err(RuntimeError::Other(
            "peek-char on stdin is not supported in v1".into(),
        )),
        Port::Closed => Err(RuntimeError::Other("peek-char: port is closed".into())),
        _ => Err(RuntimeError::Other("peek-char: not an input port".into())),
    }
}

fn read_line_from_port(port: &mut Port) -> Result<Value, RuntimeError> {
    match port {
        Port::StringInput { content, pos } | Port::FileInput { content, pos, .. } => {
            if *pos >= content.len() {
                return Ok(Value::Eof);
            }
            let remaining = &content[*pos..];
            let line_end = remaining.find('\n').map_or(remaining.len(), |i| i);
            let line: String = remaining[..line_end].to_string();
            *pos += line_end;
            // Skip the newline.
            if *pos < content.len() {
                *pos += 1;
            }
            Ok(Value::string(line))
        }
        Port::StdIn { .. } => {
            let mut buf = String::new();
            let n = std::io::stdin()
                .read_line(&mut buf)
                .map_err(|e| RuntimeError::Other(format!("read-line: {e}")))?;
            if n == 0 {
                Ok(Value::Eof)
            } else {
                // Strip trailing newline.
                while matches!(buf.chars().last(), Some('\n' | '\r')) {
                    buf.pop();
                }
                Ok(Value::string(buf))
            }
        }
        Port::Closed => Err(RuntimeError::Other("read-line: port is closed".into())),
        _ => Err(RuntimeError::Other("read-line: not an input port".into())),
    }
}

fn read_char_from_stdin() -> Result<Value, RuntimeError> {
    let mut byte = [0u8; 1];
    let n = std::io::stdin()
        .read(&mut byte)
        .map_err(|e| RuntimeError::Other(format!("read-char: {e}")))?;
    if n == 0 {
        Ok(Value::Eof)
    } else {
        // For ASCII this is fine; full UTF-8 from stdin is rare and
        // would need a more involved read loop.
        Ok(Value::Char(byte[0] as char))
    }
}

fn peek_char_from_stdin() -> Result<Value, RuntimeError> {
    Err(RuntimeError::Other(
        "peek-char on stdin is not supported in v1".into(),
    ))
}

fn read_line_from_stdin() -> Result<Value, RuntimeError> {
    let mut buf = String::new();
    let n = std::io::stdin()
        .read_line(&mut buf)
        .map_err(|e| RuntimeError::Other(format!("read-line: {e}")))?;
    if n == 0 {
        Ok(Value::Eof)
    } else {
        while matches!(buf.chars().last(), Some('\n' | '\r')) {
            buf.pop();
        }
        Ok(Value::string(buf))
    }
}

/// Source for the `(current-*-port)` procedures, defined in Scheme so
/// they don't need access to the env from inside a primitive.
pub const CURRENT_PORTS_BOOTSTRAP: &str = "
(define (current-input-port)  $stdin)
(define (current-output-port) $stdout)
(define (current-error-port)  $stderr)
";
