//! A GGUF file's name read as llama.cpp writes it: `<name>.gguf`, or
//! `<name>-<index>-of-<total>.gguf` for each shard of a split model.

/// `stem` split as llama.cpp names a shard (`<name>-<index>-of-<total>`)
/// into the name and the total; `None` for a stem that is no shard's.
pub(crate) fn shard_parts(stem: &str) -> Option<(&str, &str)> {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let (rest, total) = stem.rsplit_once("-of-")?;
    let (name, index) = rest.rsplit_once('-')?;
    (digits(index) && digits(total)).then_some((name, total))
}

/// The name the file `file_name` goes by: less a `.gguf` extension, in any
/// case, and less a shard's numbering, so every shard of a split goes by
/// one name.
pub(crate) fn gguf_stem(file_name: &str) -> &str {
    let extension = file_name.len().saturating_sub(".gguf".len());
    let stem = match file_name.get(extension..) {
        Some(tail) if tail.eq_ignore_ascii_case(".gguf") => &file_name[..extension],
        _ => file_name,
    };
    shard_parts(stem).map_or(stem, |(name, _)| name)
}

#[cfg(test)]
#[path = "shard_name_tests.rs"]
mod tests;
