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

use crate::builtins::value_to_usize;
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
    define_prim(env, "input-port-open?", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => {
            let b = p.borrow();
            Ok(Value::Bool(b.is_input() && !matches!(*b, Port::Closed)))
        }
        _ => Ok(Value::Bool(false)),
    });
    define_prim(env, "output-port-open?", Arity::Exact(1), |a| match &a[0] {
        Value::Port(p) => {
            let b = p.borrow();
            Ok(Value::Bool(b.is_output() && !matches!(*b, Port::Closed)))
        }
        _ => Ok(Value::Bool(false)),
    });
    define_prim(env, "port?", Arity::Exact(1), |a| {
        Ok(Value::Bool(matches!(a[0], Value::Port(_))))
    });
    define_prim(env, "textual-port?", Arity::Exact(1), |a| {
        Ok(Value::Bool(match &a[0] {
            Value::Port(p) => !p.borrow().is_binary(),
            _ => false,
        }))
    });
    define_prim(env, "binary-port?", Arity::Exact(1), |a| {
        Ok(Value::Bool(match &a[0] {
            Value::Port(p) => p.borrow().is_binary(),
            _ => false,
        }))
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
    define_prim(
        env,
        "open-input-bytevector",
        Arity::Exact(1),
        |a| match &a[0] {
            Value::Bytevector(b) => {
                let bytes = b.borrow().clone();
                let s = String::from_utf8(bytes).unwrap_or_else(|e| {
                    e.into_bytes().iter().map(|&b| b as char).collect()
                });
                Ok(Value::Port(Rc::new(RefCell::new(Port::BinaryInput {
                    content: s,
                    pos: 0,
                }))))
            }
            other => Err(type_err("bytevector", other)),
        },
    );
    define_prim(env, "open-output-bytevector", Arity::Exact(0), |_| {
        Ok(Value::Port(Rc::new(RefCell::new(Port::BinaryOutput {
            buffer: String::new(),
        }))))
    });
    define_prim(
        env,
        "get-output-bytevector",
        Arity::Exact(1),
        |a| match &a[0] {
            Value::Port(p) => match &*p.borrow() {
                Port::BinaryOutput { buffer } | Port::StringOutput { buffer } => {
                    Ok(Value::bytevector(buffer.as_bytes().to_vec()))
                }
                _ => Err(RuntimeError::Other(
                    "get-output-bytevector: not a bytevector output port".into(),
                )),
            },
            other => Err(type_err("port", other)),
        },
    );
    define_prim(
        env,
        "read-u8",
        Arity::Range { min: 0, max: 1 },
        |a| match a.first() {
            Some(Value::Port(p)) => {
                let mut port = p.borrow_mut();
                match &mut *port {
                    Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => {
                        let bytes = content.as_bytes();
                        if *pos >= bytes.len() {
                            return Ok(Value::Eof);
                        }
                        let b = bytes[*pos];
                        *pos += 1;
                        Ok(Value::Int(i64::from(b)))
                    }
                    _ => Err(RuntimeError::Other("read-u8: not an input port".into())),
                }
            }
            _ => Err(RuntimeError::Other("read-u8 requires an input port".into())),
        },
    );
    define_prim(
        env,
        "peek-u8",
        Arity::Range { min: 0, max: 1 },
        |a| match a.first() {
            Some(Value::Port(p)) => {
                let port = p.borrow();
                match &*port {
                    Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => {
                        let bytes = content.as_bytes();
                        if *pos >= bytes.len() {
                            return Ok(Value::Eof);
                        }
                        Ok(Value::Int(i64::from(bytes[*pos])))
                    }
                    _ => Err(RuntimeError::Other("peek-u8: not an input port".into())),
                }
            }
            _ => Err(RuntimeError::Other("peek-u8 requires an input port".into())),
        },
    );
    define_prim(env, "write-u8", Arity::Range { min: 1, max: 2 }, |a| {
        let Value::Int(n) = a[0] else {
            return Err(type_err("integer 0..255", &a[0]));
        };
        if !(0..=255).contains(&n) {
            return Err(RuntimeError::Other("write-u8: value out of range".into()));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = n as u8;
        write_to_port(a.get(1), &(b as char).to_string())
    });
    define_prim(env, "u8-ready?", Arity::Range { min: 0, max: 1 }, |_| {
        // Conservatively report ready — we don't have non-blocking I/O.
        Ok(Value::Bool(true))
    });
    // R7RS `write-bytevector bv [port [start [end]]]`: write the raw
    // bytes of `bv[start..end]` to a binary output port (interpreted
    // as the underlying String's bytes here).
    define_prim(
        env,
        "write-bytevector",
        Arity::Range { min: 1, max: 4 },
        |a| {
            let Value::Bytevector(bv) = &a[0] else {
                return Err(type_err("bytevector", &a[0]));
            };
            let bytes = bv.borrow();
            let start = if a.len() > 2 {
                value_to_usize(&a[2], "write-bytevector")?
            } else {
                0
            };
            let end = if a.len() > 3 {
                value_to_usize(&a[3], "write-bytevector")?
            } else {
                bytes.len()
            };
            if start > end || end > bytes.len() {
                return Err(RuntimeError::Other(
                    "write-bytevector: range out of bounds".into(),
                ));
            }
            let slice = &bytes[start..end];
            // Render via Latin-1 (each byte → one char) so the
            // string-backed port's UTF-8 buffer round-trips through
            // get-output-bytevector unchanged.
            let s: String = slice.iter().map(|&b| b as char).collect();
            write_to_port(a.get(1), &s)
        },
    );
    define_prim(env, "char-ready?", Arity::Range { min: 0, max: 1 }, |_| {
        Ok(Value::Bool(true))
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
                .map_err(|e| RuntimeError::FileError(format!("open-input-file({p}): {e}")))?;
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

    define_prim(env, "read", Arity::Range { min: 0, max: 1 }, |a| {
        match a.first() {
            None => Err(RuntimeError::Other(
                "read on stdin not yet supported".into(),
            )),
            Some(Value::Port(p)) => {
                let mut port = p.borrow_mut();
                read_datum_from_port(&mut port)
            }
            Some(other) => Err(type_err("port", other)),
        }
    });
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
    define_prim(env, "read-string", Arity::Range { min: 1, max: 2 }, |a| {
        let k = value_to_usize(&a[0], "read-string")?;
        match a.get(1) {
            Some(Value::Port(p)) => {
                let mut port = p.borrow_mut();
                match &mut *port {
                    Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => {
                        if *pos >= content.len() {
                            return Ok(Value::Eof);
                        }
                        let remaining = &content[*pos..];
                        let take: String = remaining.chars().take(k).collect();
                        *pos += take.len();
                        Ok(Value::string(take))
                    }
                    _ => Err(RuntimeError::Other("read-string: not an input port".into())),
                }
            }
            _ => Err(RuntimeError::Other(
                "read-string requires an input port".into(),
            )),
        }
    });
    define_prim(
        env,
        "read-bytevector",
        Arity::Range { min: 1, max: 2 },
        |a| {
            let k = value_to_usize(&a[0], "read-bytevector")?;
            match a.get(1) {
                Some(Value::Port(p)) => {
                    let mut port = p.borrow_mut();
                    match &mut *port {
                        Port::StringInput { content, pos }
                        | Port::BinaryInput { content, pos }
                        | Port::FileInput { content, pos, .. } => {
                            if *pos >= content.len() {
                                return Ok(Value::Eof);
                            }
                            let bytes = content.as_bytes();
                            let end = (*pos + k).min(bytes.len());
                            let chunk = bytes[*pos..end].to_vec();
                            *pos = end;
                            Ok(Value::bytevector(chunk))
                        }
                        _ => Err(RuntimeError::Other(
                            "read-bytevector: not an input port".into(),
                        )),
                    }
                }
                _ => Err(RuntimeError::Other(
                    "read-bytevector requires an input port".into(),
                )),
            }
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
    define_prim(env, "write-shared", Arity::Range { min: 1, max: 2 }, |a| {
        use crate::value::WriteShared;
        let s = format!("{}", WriteShared(&a[0]));
        write_to_port(a.get(1), &s)
    });
    define_prim(env, "write-simple", Arity::Range { min: 1, max: 2 }, |a| {
        // write-simple: like write but never uses datum labels. May
        // not terminate on cyclic input — that's the user's problem.
        let s = format!("{:?}", a[0]);
        write_to_port(a.get(1), &s)
    });
    define_prim(env, "newline", Arity::Range { min: 0, max: 1 }, |a| {
        write_to_port(a.first(), "\n")
    });
    define_prim(
        env,
        "flush-output-port",
        Arity::Range { min: 0, max: 1 },
        |_| {
            use std::io::Write;
            let _ = std::io::stdout().flush();
            Ok(Value::Unspecified)
        },
    );
    define_prim(
        env,
        "read-bytevector!",
        Arity::Range { min: 1, max: 4 },
        |a| {
            // (read-bytevector! bv [port [start [end]]])
            let Value::Bytevector(dest) = &a[0] else {
                return Err(type_err("bytevector", &a[0]));
            };
            let port_v = a.get(1);
            let Some(Value::Port(p)) = port_v else {
                return Err(RuntimeError::Other(
                    "read-bytevector! requires an input port".into(),
                ));
            };
            let mut port = p.borrow_mut();
            let dest_len = dest.borrow().len();
            let start = if a.len() > 2 {
                value_to_usize(&a[2], "read-bytevector!")?
            } else {
                0
            };
            let end = if a.len() > 3 {
                value_to_usize(&a[3], "read-bytevector!")?
            } else {
                dest_len
            };
            if end > dest_len || start > end {
                return Err(RuntimeError::Other(
                    "read-bytevector!: indices out of range".into(),
                ));
            }
            match &mut *port {
                Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => {
                    let bytes = content.as_bytes();
                    if *pos >= bytes.len() {
                        return Ok(Value::Eof);
                    }
                    let want = end - start;
                    let take = (bytes.len() - *pos).min(want);
                    let mut d = dest.borrow_mut();
                    d[start..start + take].copy_from_slice(&bytes[*pos..*pos + take]);
                    *pos += take;
                    #[allow(clippy::cast_possible_wrap)]
                    Ok(Value::Int(take as i64))
                }
                _ => Err(RuntimeError::Other(
                    "read-bytevector!: not an input port".into(),
                )),
            }
        },
    );
    define_prim(env, "write-char", Arity::Range { min: 1, max: 2 }, |a| {
        let Value::Char(c) = a[0] else {
            return Err(type_err("char", &a[0]));
        };
        write_to_port(a.get(1), &c.to_string())
    });
    define_prim(env, "write-string", Arity::Range { min: 1, max: 4 }, |a| {
        let Value::String(s) = &a[0] else {
            return Err(type_err("string", &a[0]));
        };
        let s_borrowed = s.borrow();
        let chars: Vec<char> = s_borrowed.chars().collect();
        let start = if a.len() > 2 {
            value_to_usize(&a[2], "write-string")?
        } else {
            0
        };
        let end = if a.len() > 3 {
            value_to_usize(&a[3], "write-string")?
        } else {
            chars.len()
        };
        if end > chars.len() || start > end {
            return Err(RuntimeError::Other(
                "write-string: bounds out of range".into(),
            ));
        }
        let slice: String = chars[start..end].iter().collect();
        write_to_port(a.get(1), &slice)
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
                .map_err(|e| RuntimeError::FileError(format!("delete-file({p}): {e}")))?;
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
                Port::StringOutput { buffer }
                | Port::BinaryOutput { buffer }
                | Port::FileOutput { buffer, .. } => {
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

/// Read one full datum from the given input port. R7RS `(read port)`.
/// Returns `eof-object` when no datum remains.
fn read_datum_from_port(port: &mut Port) -> Result<Value, RuntimeError> {
    match port {
        Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => {
            let rest = &content[*pos..];
            let (datum, consumed) = crate::parse::parse_one_with_consumed(rest)
                .map_err(|e| RuntimeError::ReadError(format!("read: {e}")))?;
            *pos += consumed;
            Ok(datum.unwrap_or(Value::Eof))
        }
        Port::StdIn { .. } => Err(RuntimeError::Other(
            "read on stdin is not supported in v1".into(),
        )),
        Port::Closed => Err(RuntimeError::Other("read: port is closed".into())),
        _ => Err(RuntimeError::Other("read: not an input port".into())),
    }
}

/// Read one character from the given input port, returning `(eof-object)`
/// at end of input.
fn read_char_from_port(port: &mut Port) -> Result<Value, RuntimeError> {
    match port {
        Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => {
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
        Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => Ok(content
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
        Port::StringInput { content, pos }
                    | Port::BinaryInput { content, pos }
                    | Port::FileInput { content, pos, .. } => {
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
