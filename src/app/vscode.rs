//! `library vscode` (spec 9.17): the project's and the installed libraries
//! as the VS Code extension's `hediet.vscode-drawio.customLibraries`.
use crate::{
    app::{libraries::Libraries, ports::EditorSettings},
    domain::{
        diagnostic::{Code, Diagnostic},
        library::Origin,
    },
};

/// The default first part of the `entryId`s this command writes.
pub const ENTRY_ID: &str = "drawio-png-cli";

/// One item of `customLibraries`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomLibrary {
    /// The extension's heading for the library; one per scope.
    pub entry_id: String,
    pub lib_name: String,
    /// `${workspaceFolder}/<path from the project root>`.
    pub file: String,
}

/// The `entryId`s this command manages: `<base>` for project libraries and
/// `<base> @<scope>` for installed ones. Other items are the user's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryIds {
    pub base: String,
}

impl EntryIds {
    pub fn new(base: Option<&str>) -> Result<Self, Diagnostic> {
        let base = base.unwrap_or(ENTRY_ID).trim();
        if base.is_empty() || base.contains('@') {
            return Err(Diagnostic::new(
                Code::Usage,
                "--entry-id must be non-empty text without '@'",
            ));
        }
        Ok(Self {
            base: base.to_owned(),
        })
    }

    pub fn manages(&self, entry_id: &str) -> bool {
        entry_id == self.base
            || entry_id
                .strip_prefix(&self.base)
                .is_some_and(|rest| rest.starts_with(" @"))
    }
}

/// The items for the project's and the installed libraries, in that order.
pub fn custom_libraries(
    libraries: &Libraries<'_>,
    ids: &EntryIds,
) -> Result<Vec<CustomLibrary>, Diagnostic> {
    let mut items = Vec::new();
    for (origin, info) in libraries.list()? {
        let entry_id = match origin {
            Origin::Project => ids.base.clone(),
            Origin::Installed => format!("{} @{}", ids.base, info.name.scope().unwrap_or("")),
        };
        let Some(location) = info.location else {
            continue;
        };
        items.push(CustomLibrary {
            entry_id,
            lib_name: info.title.unwrap_or_else(|| info.name.to_string()),
            file: format!("${{workspaceFolder}}/{location}"),
        });
    }
    Ok(items)
}

/// Writes the items into the editor's settings, keeping the user's items.
pub fn write(
    settings: &dyn EditorSettings,
    libraries: &Libraries<'_>,
    ids: &EntryIds,
) -> Result<Vec<CustomLibrary>, Diagnostic> {
    let items = custom_libraries(libraries, ids)?;
    settings.write_custom_libraries(ids, &items)?;
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_ids_of_this_command_are_recognized() {
        let ids = EntryIds::new(None).unwrap();
        assert!(ids.manages("drawio-png-cli"));
        assert!(ids.manages("drawio-png-cli @acme"));
        assert!(!ids.manages("drawio-png-cli-old"));
        assert!(!ids.manages("my icons"));
        assert!(EntryIds::new(Some("@x")).is_err());
        assert!(EntryIds::new(Some(" ")).is_err());
    }
}
