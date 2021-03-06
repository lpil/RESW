#![cfg(all(test, feature = "moz_central"))]
use resw::{
    Writer,
    write_str::WriteString,    
};
use ressa::{Parser, Error};
use flate2::read::GzDecoder;
use std::path::Path;
use rayon::prelude::*;

static mut COUNT: usize = 0;
static mut FAILURES: usize = 0;

#[test]
fn moz_central() {
    let moz_central_path = Path::new("./moz-central");
    if !moz_central_path.exists() {
        get_moz_central_test_files(&moz_central_path);
    }
    walk(&moz_central_path);
    unsafe {
        if FAILURES > 0 {
            panic!("Some spider_monkey tests failed to parse");
        }
    }
}

fn walk(path: &Path) {
    let files = path.read_dir().unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();

    files.par_iter()
        .for_each(|path| {
            unsafe {
                if COUNT > 0 && COUNT % 100 == 0 {
                    println!("Status Update {}/{}", FAILURES, COUNT);
                }
            }
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "js" {
                        match run(path) {
                            Ok(js) => {
                                if let Some(first) = js {
                                    match around_once(&first) {
                                        Ok(second) => {
                                            check_round_trips(&path, &first, &Some(second));
                                        },
                                        Err(e) => {
                                            eprintln!("{}", e);
                                            check_round_trips(&path, &first, &None);
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                let loc = match &e {
                                    Error::InvalidGetterParams(ref pos)
                                    | Error::InvalidSetterParams(ref pos)
                                    | Error::NonStrictFeatureInStrictContext(ref pos, _)
                                    | Error::OperationError(ref pos, _)
                                    | Error::Redecl(ref pos, _)
                                    | Error::UnableToReinterpret(ref pos, _, _)
                                    | Error::UnexpectedToken(ref pos, _) => format!("{}:{}:{}", path.display(), pos.line, pos.column),
                                    _ => format!("{}", path.display()),
                                };
                                eprintln!("Parse Failure {}\n\t{}", e, loc);
                                if let Ok(op) = ::std::process::Command::new("./node_modules/.bin/esparse").arg(path).output() {
                                    if !op.status.success() {
                                        eprintln!("possible new whitelist item:\n\t{}", path.display());
                                    }
                                }
                                unsafe { FAILURES += 1 }
                            }
                        }
                    }
                }
            } else {
                walk(&path)
            }
        });

}

fn run(file: &Path) -> Result<Option<String>, Error> {
    unsafe { COUNT += 1 }
    if file.ends_with("gc/bug-1459860.js")
    || file.ends_with("basic/testBug756918.js")
    || file.ends_with("basic/bug738841.js")
    || file.ends_with("ion/bug1331405.js")
    || file.ends_with("basic/testThatGenExpsActuallyDecompile.js")
    || file.ends_with("jaeger/bug672122.js")
    || file.ends_with("gc/bug-924690.js")
    || file.ends_with("auto-regress/bug732719.js")
    || file.ends_with("auto-regress/bug740509.js")
    || file.ends_with("auto-regress/bug521279.js")
    || file.ends_with("auto-regress/bug701248.js")
    || file.ends_with("auto-regress/bug1390082-1.js")
    || file.ends_with("auto-regress/bug680797.js")
    || file.ends_with("auto-regress/bug521163.js")
    || file.ends_with("auto-regress/bug1448582-5.js")
    || file.ends_with("tests/backup-point-bug1315634.js")
    || file.ends_with("auto-regress/bug650574.js")
    || file.ends_with("baseline/setcall.js") {
        return Ok(None)
    }
    let contents = ::std::fs::read_to_string(file)?;
    if contents.starts_with("// |jit-test| error: SyntaxError")
        || contents.starts_with("|")
        || contents.starts_with("// |jit-test| error:SyntaxError") {
        return Ok(None);
    }
    if contents.starts_with("// |jit-test| module") {
        return Ok(None); //these all contain restricted word import as an ident
    }
    let ret = around_once(&contents)?;
    Ok(Some(ret))
}

fn around_once(js: &str) -> Result<String, Error> {
    let mut out = WriteString::new();
    let mut writer = Writer::new(out.generate_child());
    for part in Parser::new(&js)? {
        let part = part?;
        writer.write_part(&part).expect("Failed to write part");
    }
    Ok(out.get_string().expect("invalid utf8 written to write_string"))
}

fn get_moz_central_test_files(path: &Path) {
    let mut response = reqwest::get("https://hg.mozilla.org/mozilla-central/archive/tip.tar.gz/js/src/jit-test/tests/")
        .expect("Failed to get zip of moz-central");
    let mut buf = Vec::new();
    response.copy_to(&mut buf)
        .expect("failed to copy to BzDecoder");
    let gz = GzDecoder::new(buf.as_slice());
    let mut t = tar::Archive::new(gz);
    t.unpack(path).expect("Failed to unpack gz");
}
fn check_round_trips(path: &Path, first: &str, second: &Option<String>) {
    let name = path.file_name().unwrap().to_str().unwrap();
    if let Some(ref js) = second {
        if first != js {
            write_failure(name, first, second);
            eprintln!("Double round trip failed for {0}\ncheck ./{1}.first.js and ./{1}.second.js", path.display(), name);
            unsafe { FAILURES += 1}
        }
    } else {
        write_failure(name, first, second);
        eprintln!("Double round trip failed to parse second pass for {}\n chec ./{}.first.js", path.display(), name);
        unsafe { FAILURES += 1}
    }
}
fn write_failure(name: &str, first: &str, second: &Option<String>) {
    use std::io::Write;
    let dir = ::std::path::PathBuf::from("test_failures");
    if !dir.exists() {
        ::std::fs::create_dir(&dir).expect("failed to create test_failures");
    }
    let mut f1 = ::std::fs::File::create(dir.join(&format!("{}.first.js", name))).expect("Failed to create first failure file");
    f1.write(format!("//{}\n", name).as_bytes()).expect("Failed to write first line");
    f1.write_all(first.as_bytes()).expect("failed to write to first failure file");
    if let Some(ref second) = second {
        let mut f2 = ::std::fs::File::create(dir.join(&format!("{}.second.js", name))).expect("Failed to create second failure file");
        f2.write(format!("//{}\n", name).as_bytes()).expect("Failed to write first line");
        f2.write_all(second.as_bytes()).expect("failed to write second failure file");
    }
}