use anyhow::Result;
use clap::{Parser, Subcommand};
use drawio_png_cli::{
    chromium::ChromiumMode, document, fonts, library, png_data, render, shapes, storage,
};
use std::{
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    name = "drawio-png-cli",
    version,
    about = "Edit draw.io diagrams embedded in PNG images"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print licenses and notices for drawio-png-cli, dependencies, and bundled assets
    Licenses,
    /// Print the agent skill guide for this version of drawio-png-cli (vscode mode)
    Skill {
        /// Print the full reference instead (Desktop, raw and desktop modes, fonts, --no-render)
        #[arg(long)]
        full: bool,
    },
    /// Extract editable, uncompressed XML from a draw.io PNG
    Extract {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Embed XML into a PNG, rendering its first page unless --no-render is set
    #[command(
        after_help = "Renderer environment:\n  DRAWIO_PNG_DRAWIO_PATH      Desktop executable\n  DRAWIO_PNG_DRAWIO_ARGS      Additional Desktop options\n  DRAWIO_PNG_CHROME_PATH      Chromium or Chrome executable (or CHROME_PATH)\n  DRAWIO_PNG_CHROMIUM_MODE    raw, desktop, or vscode (default: vscode)\n  DRAWIO_PNG_CHROME_ARGS      Additional POSIX-quoted options (no shell expansion)\n  DRAWIO_PNG_DRAWIO_WEB_PATH  Local draw.io webapp directory (default: bundled assets)"
    )]
    Embed {
        /// XML file (omit to read stdin)
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        /// Preserve this PNG's pixels; requires --no-render
        #[arg(short, long, requires = "no_render")]
        base_image: Option<PathBuf>,
        /// Update metadata only; create a transparent 1x1 PNG if no base is given
        #[arg(long)]
        no_render: bool,
        /// Rendering backend (auto prefers Desktop, then Chromium)
        #[arg(long, value_enum, conflicts_with = "no_render")]
        renderer: Option<render::Renderer>,
        /// Chromium output mode (overrides DRAWIO_PNG_CHROMIUM_MODE; default: vscode)
        #[arg(long, value_enum, conflicts_with = "no_render")]
        chromium_mode: Option<ChromiumMode>,
        /// Allow external HTTP(S) images and fonts in Chromium
        #[arg(long, conflicts_with = "no_render")]
        allow_network: bool,
        /// Set and save the font for cells without an explicit fontFamily
        #[arg(long, value_parser = fonts::parse_family, conflicts_with = "no_validate")]
        default_font: Option<String>,
        /// Append a fallback family (repeatable, in priority order); requires --default-font
        #[arg(long, value_parser = fonts::parse_family, requires = "default_font")]
        fallback_font: Vec<String>,
        /// Debug only: skip draw.io XML validation
        #[arg(long)]
        no_validate: bool,
    },
    /// Validate XML or a draw.io PNG (all pages)
    Validate { input: PathBuf },
    /// Insert a library entry (including draw.io's built-in shapes) into a page of an uncompressed draw.io XML file; prints the new cell IDs
    #[command(after_help = LIBRARY_ENV)]
    Insert {
        #[command(flatten)]
        source: LibrarySource,
        /// Entry title (exact, or a unique case-insensitive match)
        #[arg(long, required_unless_present = "index", conflicts_with = "index")]
        name: Option<String>,
        /// Entry index in its library as shown by `library search` (starting at 1)
        #[arg(long)]
        index: Option<usize>,
        /// XML file (omit to read stdin)
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        /// Page number, starting at 1
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// Left edge of the inserted shapes
        #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
        x: f64,
        /// Top edge of the inserted shapes
        #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
        y: f64,
        /// Width of a single-cell entry (keeps the aspect ratio if --height is omitted)
        #[arg(long)]
        width: Option<f64>,
        /// Height of a single-cell entry (keeps the aspect ratio if --width is omitted)
        #[arg(long)]
        height: Option<f64>,
        /// ID for the top-level cell; other cells get "<ID>-<n>"
        #[arg(long)]
        id: Option<String>,
        /// Label of the top-level cell
        #[arg(long)]
        label: Option<String>,
    },
    /// Inspect draw.io custom libraries and the built-in shape libraries (drawio/*) without changing anything
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
}

const LIBRARY_ENV: &str = "Libraries:\n  DRAWIO_PNG_LIBRARY_PATH  Library files or directories of *.xml files, separated\n                           like PATH; used unless --library-file is given\n  drawio/*                 Built-in libraries of draw.io 26.0.2's sidebar shapes\n                           (AWS, Azure, Google Cloud, Kubernetes, UML, ...), listed after\n                           DRAWIO_PNG_LIBRARY_PATH; rendering most of them needs\n                           DRAWIO_PNG_DRAWIO_WEB_PATH";

/// Which libraries to use.
#[derive(clap::Args)]
struct LibrarySource {
    /// Library name: its file name without .xml, or drawio/<name> for a built-in one
    #[arg(long)]
    library: Option<String>,
    /// Library file to use instead of DRAWIO_PNG_LIBRARY_PATH and the built-in libraries
    #[arg(long, value_name = "PATH", conflicts_with = "library")]
    library_file: Option<PathBuf>,
    /// Leave out the built-in libraries (drawio/*)
    #[arg(long, conflicts_with = "library_file")]
    no_builtin: bool,
}

#[derive(Subcommand)]
enum LibraryCommand {
    /// List libraries as name, entry count and title
    #[command(visible_alias = "ls", after_help = LIBRARY_ENV)]
    List {
        /// Library file to use instead of DRAWIO_PNG_LIBRARY_PATH and the built-in libraries
        #[arg(long, value_name = "PATH")]
        library_file: Option<PathBuf>,
        /// Leave out the built-in libraries (drawio/*)
        #[arg(long, conflicts_with = "library_file")]
        no_builtin: bool,
    },
    /// Show details of one library
    #[command(after_help = LIBRARY_ENV)]
    Show {
        /// Library name: its file name without .xml, or drawio/<name> for a built-in one
        name: String,
        /// Library file to use instead of DRAWIO_PNG_LIBRARY_PATH and the built-in libraries
        #[arg(long, value_name = "PATH")]
        library_file: Option<PathBuf>,
    },
    /// Search entries; prints library, index, title and size, without their content
    #[command(after_help = LIBRARY_ENV)]
    Search {
        /// Text the title must contain (case-insensitive); omit to list every entry
        query: Option<String>,
        #[command(flatten)]
        source: LibrarySource,
    },
    /// Print the style of a single-cell entry, escaped to paste into a style="..." attribute
    #[command(after_help = LIBRARY_ENV)]
    Style {
        /// Library name, as shown by `library search`
        library: String,
        /// Entry index in the library, as shown by `library search` (starting at 1)
        index: usize,
    },
    /// Render matching entries, labelled with library, index and title, into a PNG
    #[command(
        after_help = "Renders with Chromium in vscode mode, at most 60 entries. Built-in shapes (drawio/*)\nmostly need draw.io 26.0.2 web assets in DRAWIO_PNG_DRAWIO_WEB_PATH.\n\nLibraries:\n  DRAWIO_PNG_LIBRARY_PATH  Library files or directories of *.xml files, separated\n                           like PATH; used unless --library-file is given"
    )]
    Preview {
        /// Text the title must contain (case-insensitive)
        #[arg(required_unless_present = "library")]
        query: Option<String>,
        #[command(flatten)]
        source: LibrarySource,
        /// PNG file to write
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Skill { full } => print(if full {
            include_str!("../assets/skill/drawio-png-full.md")
        } else {
            include_str!("../assets/skill/drawio-png.md")
        })?,
        Command::Licenses => {
            io::stdout()
                .lock()
                .write_all(drawio_png_cli::BUNDLED_LICENSES.as_bytes())?;
        }
        Command::Extract { input, output } => {
            let data = storage::read(&input)?;
            let xml = document::normalize(&png_data::extract(&data)?)?;
            if let Some(path) = output {
                storage::atomic_write(&path, xml.as_bytes())?;
            } else {
                io::stdout().lock().write_all(xml.as_bytes())?;
            }
        }
        Command::Embed {
            input,
            output,
            base_image,
            no_render,
            no_validate,
            renderer,
            chromium_mode,
            allow_network,
            default_font,
            fallback_font,
        } => {
            let bytes = if let Some(path) = input {
                storage::read(&path)?
            } else {
                storage::read_limited(io::stdin().lock())?
            };
            let xml = document::utf8(&bytes)?;
            let xml = if no_validate {
                eprintln!("warning: draw.io XML validation is disabled");
                xml.to_owned()
            } else {
                document::validate(xml)?;
                document::normalize(xml)?
            };
            let xml = if let Some(default) = default_font {
                fonts::FontPolicy::new(&default, &fallback_font)?.apply(&xml)?
            } else {
                xml
            };
            let base = if no_render {
                eprintln!(
                    "warning: PNG pixels are not synchronized with the diagram (--no-render)"
                );
                if let Some(path) = base_image {
                    storage::read(&path)?
                } else {
                    png_data::transparent()?
                }
            } else {
                render::render_selected_with_mode(
                    &xml,
                    renderer.unwrap_or_default(),
                    allow_network,
                    chromium_mode,
                )?
            };
            let png = png_data::embed(&base, &xml)?;
            storage::atomic_write(&output, &png)?;
        }
        Command::Validate { input } => {
            let data = storage::read(&input)?;
            let xml = if data.starts_with(png_data::SIGNATURE) {
                png_data::extract(&data)?
            } else {
                document::utf8(&data)?.to_owned()
            };
            document::validate(&xml)?;
        }
        Command::Library { command } => library_command(command)?,
        Command::Insert {
            source,
            name,
            index,
            input,
            output,
            page,
            x,
            y,
            width,
            height,
            id,
            label,
        } => insert(
            source,
            name,
            index,
            input,
            output,
            library::Placement {
                page,
                x,
                y,
                width,
                height,
                id: id.as_deref(),
                label: label.as_deref(),
            },
        )?,
    }
    Ok(())
}

/// The libraries to use: the given file, or those on DRAWIO_PNG_LIBRARY_PATH followed
/// by the built-in ones unless `builtin` is false, optionally only the one with
/// the given name. File-based names never contain '/', so an exact name is
/// unambiguous; a built-in library may be named without its drawio/ prefix
/// when no library has that exact name.
fn libraries(
    file: Option<PathBuf>,
    name: Option<&str>,
    builtin: bool,
) -> Result<Vec<library::Library>> {
    let libraries = if let Some(file) = file {
        vec![library::open(&file)?]
    } else {
        let path = std::env::var_os("DRAWIO_PNG_LIBRARY_PATH").filter(|p| !p.is_empty());
        anyhow::ensure!(
            path.is_some() || builtin,
            "set DRAWIO_PNG_LIBRARY_PATH or pass --library-file"
        );
        let mut libraries = Vec::new();
        if let Some(path) = path {
            let (found, warnings) = library::search(&path)?;
            for warning in warnings {
                eprintln!("warning: skipped {warning}");
            }
            libraries = found;
        }
        if builtin {
            libraries.extend(shapes::libraries()?);
        }
        libraries
    };
    let Some(name) = name else {
        anyhow::ensure!(
            !libraries.is_empty(),
            "no libraries found in DRAWIO_PNG_LIBRARY_PATH"
        );
        return Ok(libraries);
    };
    let prefixed = format!("{}{name}", shapes::PREFIX);
    let has = |n: &str| libraries.iter().any(|l| l.name == n);
    let chosen = match (has(name), has(&prefixed)) {
        (true, _) => name,
        (false, true) => prefixed.as_str(),
        (false, false) => {
            let mut available: Vec<_> = libraries
                .iter()
                .filter(|l| !shapes::is_builtin(l))
                .map(|l| l.name.clone())
                .collect();
            if libraries.iter().any(shapes::is_builtin) {
                available.push(format!(
                    "{}* (see `drawio-png-cli library list`)",
                    shapes::PREFIX
                ));
            }
            anyhow::bail!(
                "no library named {name:?}; available: {}",
                available.join(", ")
            )
        }
    }
    .to_owned();
    Ok(libraries.into_iter().filter(|l| l.name == chosen).collect())
}

/// A size for display, without floating-point noise such as 50.099999999999994.
fn size(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Escapes text for a double-quoted XML attribute, such as styles with JSON values.
fn attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Keeps one record per line.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Writes to stdout; a consumer such as `head` may stop reading early.
fn print(text: &str) -> Result<()> {
    match io::stdout().lock().write_all(text.as_bytes()) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

fn library_command(command: LibraryCommand) -> Result<()> {
    let mut out = String::new();
    match command {
        LibraryCommand::List {
            library_file,
            no_builtin,
        } => {
            for library in libraries(library_file, None, !no_builtin)? {
                out.push_str(&format!(
                    "{}\t{}\t{}\n",
                    library.name,
                    library.entries.len(),
                    library.title.as_deref().map_or("-".into(), one_line)
                ));
            }
        }
        LibraryCommand::Show { name, library_file } => {
            for library in libraries(library_file, Some(&name), true)? {
                let file = if shapes::is_builtin(&library) {
                    "(built in: draw.io 26.0.2 sidebar)".to_owned()
                } else {
                    library.path.display().to_string()
                };
                out.push_str(&format!(
                    "name: {}\ntitle: {}\nfile: {file}\nentries: {}\n",
                    library.name,
                    library.title.as_deref().map_or("-".into(), one_line),
                    library.entries.len()
                ));
            }
        }
        LibraryCommand::Search { query, source } => {
            let libraries = source.libraries()?;
            for (library, index) in matches(&libraries, query.as_deref()) {
                let entry = &library.entries[index];
                out.push_str(&format!(
                    "{}\t{}\t{}\t{}x{}\n",
                    library.name,
                    index + 1,
                    one_line(&entry.title),
                    size(entry.width),
                    size(entry.height)
                ));
            }
        }
        LibraryCommand::Style { library, index } => {
            let libraries = libraries(None, Some(&library), true)?;
            let (library, entry) = entry_at(&libraries, index)?;
            let style = entry.style()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "{} #{index} is not a single cell with a plain style (several cells or image data); \
                     add it with `drawio-png-cli insert --library {} --index {index}`",
                    library.name,
                    library.name
                )
            })?;
            out = format!("{}\n", attribute(&one_line(&style)));
        }
        LibraryCommand::Preview {
            query,
            source,
            output,
        } => {
            let libraries = source.libraries()?;
            let found = matches(&libraries, query.as_deref());
            anyhow::ensure!(
                !found.is_empty(),
                "no entries match; see `drawio-png-cli library search`"
            );
            let xml = shapes::preview(&found)?;
            let png = render::render_selected_with_mode(
                &xml,
                render::Renderer::Chromium,
                false,
                Some(ChromiumMode::Vscode),
            )?;
            storage::atomic_write(&output, &png)?;
        }
    }
    print(&out)
}

impl LibrarySource {
    fn libraries(&self) -> Result<Vec<library::Library>> {
        libraries(
            self.library_file.clone(),
            self.library.as_deref(),
            !self.no_builtin,
        )
    }
}

/// Entries whose title contains `query` (case-insensitive), as (library, 0-based index).
fn matches<'a>(
    libraries: &'a [library::Library],
    query: Option<&str>,
) -> Vec<(&'a library::Library, usize)> {
    let query = query.map(str::to_lowercase);
    libraries
        .iter()
        .flat_map(|l| (0..l.entries.len()).map(move |i| (l, i)))
        .filter(|(l, i)| {
            query
                .as_ref()
                .is_none_or(|q| l.entries[*i].title.to_lowercase().contains(q))
        })
        .collect()
}

/// The entry at a 1-based `index` of the only library given.
fn entry_at<'a>(
    libraries: impl IntoIterator<Item = &'a library::Library>,
    index: usize,
) -> Result<(&'a library::Library, &'a library::Entry)> {
    let libraries: Vec<_> = libraries.into_iter().collect();
    let [library] = libraries[..] else {
        anyhow::bail!("--index needs one library; use --library");
    };
    let entry = index
        .checked_sub(1)
        .and_then(|i| library.entries.get(i))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "library {:?} has {} entries; --index starts at 1",
                library.name,
                library.entries.len()
            )
        })?;
    Ok((library, entry))
}

fn insert(
    source: LibrarySource,
    name: Option<String>,
    index: Option<usize>,
    input: Option<PathBuf>,
    output: PathBuf,
    placement: library::Placement<'_>,
) -> Result<()> {
    let libraries = source.libraries()?;
    // A title on DRAWIO_PNG_LIBRARY_PATH wins over the built-in libraries, as before they existed.
    let (custom, builtin): (Vec<_>, Vec<_>) =
        libraries.into_iter().partition(|l| !shapes::is_builtin(l));
    let libraries: Vec<_> = custom.iter().chain(&builtin).collect();
    let entry = match (name, index) {
        (Some(name), _) => {
            let lower = name.to_lowercase();
            let in_custom = custom
                .iter()
                .any(|l| l.entries.iter().any(|e| e.title.to_lowercase() == lower));
            library::find(if in_custom { &custom } else { &builtin }, &name)?.1
        }
        (None, Some(index)) => entry_at(libraries, index)?.1,
        (None, None) => unreachable!("clap requires --name or --index"),
    };
    for (flag, value) in [
        ("--x", Some(placement.x)),
        ("--y", Some(placement.y)),
        ("--width", placement.width),
        ("--height", placement.height),
    ] {
        anyhow::ensure!(
            value.is_none_or(f64::is_finite),
            "{flag} must be a finite number"
        );
    }
    for (flag, value) in [("--width", placement.width), ("--height", placement.height)] {
        anyhow::ensure!(value.is_none_or(|v| v > 0.0), "{flag} must be positive");
    }
    let bytes = if let Some(path) = input {
        storage::read(&path)?
    } else {
        storage::read_limited(io::stdin().lock())?
    };
    let (xml, ids) = library::insert(document::utf8(&bytes)?, entry, &placement)?;
    storage::atomic_write(&output, xml.as_bytes())?;
    print(&ids.iter().map(|id| format!("{id}\n")).collect::<String>())
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
