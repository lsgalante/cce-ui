use std::path::PathBuf;

/// Opens a portal file chooser dialog for selecting a file.
pub fn pick_file(title: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    for (name, exts) in filters {
        dialog = dialog.add_filter(*name, *exts);
    }
    dialog.pick_file()
}

/// Opens a portal file chooser dialog for saving a file.
pub fn save_file(title: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    for (name, exts) in filters {
        dialog = dialog.add_filter(*name, *exts);
    }
    dialog.save_file()
}
