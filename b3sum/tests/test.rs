use duct::cmd;
use std::ffi::OsString;
use std::fs;
use std::io::prelude::*;
use std::path::PathBuf;

pub fn b3sum_exe() -> PathBuf {
    assert_cmd::cargo::cargo_bin("b3sum")
}

#[test]
fn test_hash_one() {
    let expected = blake3::hash(b"foo").to_hex();
    let output = cmd!(b3sum_exe()).stdin_bytes("foo").read().unwrap();
    assert_eq!(&*expected, output);
}

#[test]
fn test_hash_one_raw() {
    let expected = blake3::hash(b"foo").as_bytes().to_owned();
    let output = cmd!(b3sum_exe(), "--raw")
        .stdin_bytes("foo")
        .stdout_capture()
        .run()
        .unwrap()
        .stdout;
    assert_eq!(expected, output.as_slice());
}

#[test]
fn test_hash_many() {
    let dir = tempfile::tempdir().unwrap();
    let file1 = dir.path().join("file1");
    fs::write(&file1, b"foo").unwrap();
    let file2 = dir.path().join("file2");
    fs::write(&file2, b"bar").unwrap();

    let output = cmd!(b3sum_exe(), &file1, &file2).read().unwrap();
    let foo_hash = blake3::hash(b"foo");
    let bar_hash = blake3::hash(b"bar");
    let expected = format!(
        "{}  {}\n{}  {}",
        foo_hash.to_hex(),
        // account for slash normalization on Windows
        file1.to_string_lossy().replace("\\", "/"),
        bar_hash.to_hex(),
        file2.to_string_lossy().replace("\\", "/"),
    );
    assert_eq!(expected, output);

    let output_no_names = cmd!(b3sum_exe(), "--no-names", &file1, &file2)
        .read()
        .unwrap();
    let expected_no_names = format!("{}\n{}", foo_hash.to_hex(), bar_hash.to_hex(),);
    assert_eq!(expected_no_names, output_no_names);
}

#[test]
fn test_hash_length() {
    let mut buf = [0; 100];
    blake3::Hasher::new()
        .update(b"foo")
        .finalize_xof()
        .fill(&mut buf);
    let expected = hex::encode(&buf[..]);
    let output = cmd!(b3sum_exe(), "--length=100")
        .stdin_bytes("foo")
        .read()
        .unwrap();
    assert_eq!(&*expected, &*output);
}

#[test]
fn test_keyed() {
    let key = [42; blake3::KEY_LEN];
    let f = tempfile::NamedTempFile::new().unwrap();
    f.as_file().write_all(b"foo").unwrap();
    f.as_file().flush().unwrap();
    let expected = blake3::keyed_hash(&key, b"foo").to_hex();
    let output = cmd!(b3sum_exe(), "--keyed", "--no-names", f.path())
        .stdin_bytes(&key[..])
        .read()
        .unwrap();
    assert_eq!(&*expected, &*output);
}

#[test]
fn test_derive_key() {
    let context = "BLAKE3 2019-12-28 10:28:41 example context";
    let f = tempfile::NamedTempFile::new().unwrap();
    f.as_file().write_all(b"key material").unwrap();
    f.as_file().flush().unwrap();
    let mut derive_key_out = [0; blake3::OUT_LEN];
    blake3::derive_key(context, b"key material", &mut derive_key_out);
    let expected = hex::encode(&derive_key_out);
    let output = cmd!(b3sum_exe(), "--derive-key", context, "--no-names", f.path())
        .read()
        .unwrap();
    assert_eq!(&*expected, &*output);
}

#[test]
fn test_no_mmap() {
    let f = tempfile::NamedTempFile::new().unwrap();
    f.as_file().write_all(b"foo").unwrap();
    f.as_file().flush().unwrap();

    let expected = blake3::hash(b"foo").to_hex();
    let output = cmd!(b3sum_exe(), "--no-mmap", "--no-names", f.path())
        .read()
        .unwrap();
    assert_eq!(&*expected, &*output);
}

#[test]
fn test_length_without_value_is_an_error() {
    let result = cmd!(b3sum_exe(), "--length")
        .stdin_bytes("foo")
        .stderr_capture()
        .run();
    assert!(result.is_err());
}

#[test]
fn test_raw_with_multi_files_is_an_error() {
    let f1 = tempfile::NamedTempFile::new().unwrap();
    let f2 = tempfile::NamedTempFile::new().unwrap();

    // Make sure it doesn't error with just one file
    let result = cmd!(b3sum_exe(), "--raw", f1.path()).stdout_capture().run();
    assert!(result.is_ok());

    // Make sure it errors when both file are passed
    let result = cmd!(b3sum_exe(), "--raw", f1.path(), f2.path())
        .stderr_capture()
        .run();
    assert!(result.is_err());
}

#[test]
#[cfg(unix)]
fn test_newline_and_backslash_escaping_on_unix() {
    let empty_hash = blake3::hash(b"").to_hex();
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();
    let names = [
        "abcdef",
        "abc\ndef",
        "abc\\def",
        "abc\rdef",
        "abc\r\ndef",
        "subdir/foo",
    ];
    let mut paths = Vec::new();
    for name in &names {
        let path = dir.path().join(name);
        println!("creating file at {:?}", path);
        fs::write(&path, b"").unwrap();
        paths.push(path);
    }
    let output = cmd(b3sum_exe(), &names).dir(dir.path()).read().unwrap();
    let expected = format!(
        "\
{0}  abcdef
\\{0}  abc\\ndef
\\{0}  abc\\\\def
{0}  abc\rdef
\\{0}  abc\r\\ndef
{0}  subdir/foo",
        empty_hash,
    );
    println!("output");
    println!("======");
    println!("{}", output);
    println!();
    println!("expected");
    println!("========");
    println!("{}", expected);
    println!();
    assert_eq!(expected, output);
}

#[test]
#[cfg(windows)]
fn test_slash_normalization_on_windows() {
    let empty_hash = blake3::hash(b"").to_hex();
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();
    // Note that filenames can't contain newlines or backslashes on Windows, so
    // we don't test escaping here. We only test forward slash and backslash as
    // directory separators.
    let names = ["abcdef", "subdir/foo", "subdir\\bar"];
    let mut paths = Vec::new();
    for name in &names {
        let path = dir.path().join(name);
        println!("creating file at {:?}", path);
        fs::write(&path, b"").unwrap();
        paths.push(path);
    }
    let output = cmd(b3sum_exe(), &names).dir(dir.path()).read().unwrap();
    let expected = format!(
        "\
{0}  abcdef
{0}  subdir/foo
{0}  subdir/bar",
        empty_hash,
    );
    println!("output");
    println!("======");
    println!("{}", output);
    println!();
    println!("expected");
    println!("========");
    println!("{}", expected);
    println!();
    assert_eq!(expected, output);
}

#[test]
#[cfg(unix)]
fn test_invalid_unicode_on_unix() {
    use std::os::unix::ffi::OsStringExt;

    let empty_hash = blake3::hash(b"").to_hex();
    let dir = tempfile::tempdir().unwrap();
    let names = ["abcdef".into(), OsString::from_vec(b"abc\xffdef".to_vec())];
    let mut paths = Vec::new();
    for name in &names {
        let path = dir.path().join(name);
        println!("creating file at {:?}", path);
        // Note: Some operating systems, macOS in particular, simply don't
        // allow invalid Unicode in filenames. On those systems, this write
        // will fail. That's fine, we'll just short-circuit this test in that
        // case. But assert that at least Linux allows this.
        let write_result = fs::write(&path, b"");
        if cfg!(target_os = "linux") {
            write_result.expect("Linux should allow invalid Unicode");
        } else if write_result.is_err() {
            return;
        }
        paths.push(path);
    }
    let output = cmd(b3sum_exe(), &names).dir(dir.path()).read().unwrap();
    let expected = format!(
        "\
{0}  abcdef
{0}  abc�def",
        empty_hash,
    );
    println!("output");
    println!("======");
    println!("{}", output);
    println!();
    println!("expected");
    println!("========");
    println!("{}", expected);
    println!();
    assert_eq!(expected, output);
}

#[test]
#[cfg(windows)]
fn test_invalid_unicode_on_windows() {
    use std::os::windows::ffi::OsStringExt;

    let empty_hash = blake3::hash(b"").to_hex();
    let dir = tempfile::tempdir().unwrap();
    let surrogate_char = 0xDC00;
    let bad_unicode_wchars = [
        'a' as u16,
        'b' as u16,
        'c' as u16,
        surrogate_char,
        'd' as u16,
        'e' as u16,
        'f' as u16,
    ];
    let bad_osstring = OsString::from_wide(&bad_unicode_wchars);
    let names = ["abcdef".into(), bad_osstring];
    let mut paths = Vec::new();
    for name in &names {
        let path = dir.path().join(name);
        println!("creating file at {:?}", path);
        fs::write(&path, b"").unwrap();
        paths.push(path);
    }
    let output = cmd(b3sum_exe(), &names).dir(dir.path()).read().unwrap();
    let expected = format!(
        "\
{0}  abcdef
{0}  abc�def",
        empty_hash,
    );
    println!("output");
    println!("======");
    println!("{}", output);
    println!();
    println!("expected");
    println!("========");
    println!("{}", expected);
    println!();
    assert_eq!(expected, output);
}
