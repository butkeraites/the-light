//! Cópia para a área de transferência do sistema, **sem dependências externas**.
//!
//! Estratégia em camadas: tenta o utilitário nativo do SO (`pbcopy` no macOS,
//! `wl-copy`/`xclip`/`xsel` no Linux, `clip` no Windows) e, se nenhum existir ou
//! falhar, cai para a sequência OSC 52 — aceita por terminais como iTerm2 e útil
//! sobre SSH. Sob tmux/screen ela exige passthrough habilitado (`set-clipboard`),
//! então no macOS o caminho primário (`pbcopy`) é o confiável. `true` significa
//! "enviado com sucesso", não "verificado na área de transferência".

use std::io::Write;
use std::process::{Command, Stdio};

/// Copia `text` para a área de transferência. Best-effort.
pub fn copy(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if copy_via_command(text) {
        return true;
    }
    copy_via_osc52(text)
}

/// Tenta os utilitários de clipboard do SO, na ordem de preferência.
fn copy_via_command(text: &str) -> bool {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["-ib"]),
        ]
    };
    candidates
        .iter()
        .any(|(cmd, args)| try_pipe(cmd, args, text))
}

/// Roda `cmd args`, escrevendo `text` no stdin. `true` se encerrou com sucesso.
fn try_pipe(cmd: &str, args: &[&str], text: &str) -> bool {
    let spawned = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(text.as_bytes()).is_err() {
            let _ = child.wait();
            return false;
        }
    }
    // Fecha o stdin (drop) antes de aguardar; senão o processo nunca termina.
    drop(child.stdin.take());
    matches!(child.wait(), Ok(status) if status.success())
}

/// Emite a sequência OSC 52 (clipboard `c`), com o conteúdo em base64.
fn copy_via_osc52(text: &str) -> bool {
    let seq = format!("\x1b]52;c;{}\x07", base64_encode(text.as_bytes()));
    let mut out = std::io::stdout();
    out.write_all(seq.as_bytes()).is_ok() && out.flush().is_ok()
}

/// Base64 padrão (RFC 4648) — o suficiente para o payload do OSC 52.
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_matches_known_vectors() {
        // Vetores clássicos do RFC 4648.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_handles_utf8_payload() {
        // "graça" tem um caractere multibyte; a codificação opera nos bytes.
        assert_eq!(base64_encode("graça".as_bytes()), "Z3Jhw6dh");
    }
}
