//! The browser owns cloud integration; drawing codecs and local saves remain upstream-owned.
use sovereign_cloud_ui::{DocumentState, Editor, Snapshot};

pub struct Adapter<'a>(pub &'a mut cadcraft_ui_egui::CadApp);
impl Editor for Adapter<'_> {
    fn document(&self) -> Option<DocumentState> {
        self.0.session.state().ok().map(|d| DocumentState { id: d.uid, revision: d.revision, name: d.title.clone(), dirty: d.is_dirty() })
    }
    fn capture(&self) -> Result<Snapshot, String> {
        let document = self.document().ok_or("No drawing is open")?;
        let doc = self.0.session.state().map_err(|e| e.to_string())?.doc.clone();
        Ok(Snapshot { document, encode: Box::new(move || cadcraft_io::write(&doc, "drawing.dxf").map_err(|e| e.to_string())) })
    }
    fn saved(&mut self, id: u64, revision: u64, _bytes: &[u8]) -> Result<Option<u64>, String> {
        if let Ok(d) = self.0.session.state_mut()
            && d.uid == id
            && d.revision == revision
        {
            d.saved = d.doc.clone();
            return Ok(Some(revision));
        }
        Ok(None)
    }
    fn open(&mut self, name: &str, bytes: &[u8]) -> Result<DocumentState, String> {
        // Parse completely before adding a new tab, preserving existing unsaved drawings.
        let doc = cadcraft_io::read(bytes, "drawing.dxf").map_err(|e| e.to_string())?;
        self.0.session.open_drawing(doc, name, None);
        self.0.ui.start_tab = false;
        self.0.canvas.zoom_pending = true;
        self.document().ok_or_else(|| "Could not open the drawing".into())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn cloud_snapshot_roundtrips_and_cannot_clear_newer_edits() {
        let mut app = cadcraft_ui_egui::CadApp::new(cadcraft_engine::Session::new(), Default::default());
        app.session.execute("line", &json!({"points": [[0,0],[10,20]]})).unwrap();
        let snapshot = Adapter(&mut app).capture().unwrap();
        let original = snapshot.document;
        app.session.execute("circle", &json!({"center": [5,5], "radius": 2})).unwrap();
        let bytes = (snapshot.encode)().unwrap();
        assert_eq!(cadcraft_io::read(&bytes, "drawing.dxf").unwrap().entity_count(), 1);
        assert_eq!(Adapter(&mut app).saved(original.id, original.revision, &bytes).unwrap(), None);
        assert!(app.session.state().unwrap().is_dirty());
        let count = app.session.docs.len();
        assert!(Adapter(&mut app).open("bad.dxf", b"invalid drawing").is_err());
        assert_eq!(app.session.docs.len(), count);
        let opened = Adapter(&mut app).open("Drawing.dxf", &bytes).unwrap();
        assert_ne!(opened.id, original.id);
        assert_eq!(app.session.state().unwrap().doc.entity_count(), 1);
        assert!(app.session.docs.first().unwrap().is_dirty());
        assert!(!opened.dirty);
        app.session.execute("circle", &json!({"center": [2,2], "radius": 1})).unwrap();
        let current = Adapter(&mut app).document().unwrap();
        assert_eq!(Adapter(&mut app).saved(current.id, current.revision, &[]).unwrap(), Some(current.revision));
        assert!(!app.session.state().unwrap().is_dirty());
    }
}
