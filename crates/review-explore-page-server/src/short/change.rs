use crate::changed_source::{ChangedSource, FixedChange};

const OLD: &str = "\
pub struct Round {
    pub id: RoundId,
}

impl Round {
    pub fn reopen(&mut self) {
        self.draft = None;
    }
}
";

const NEW: &str = "\
pub struct Round {
    pub id: RoundId,
    pub draft: Option<Draft>,
}

impl Round {
    pub fn reopen(&mut self) {
        // The unsent draft stays with the round, so that an answer the reviewer typed before closing the pane is still there when it opens again.
    }
}
";

const DIFF: &str = "\
diff --git a/src/drafts.rs b/src/drafts.rs
--- a/src/drafts.rs
+++ b/src/drafts.rs
@@ -1,9 +1,10 @@
 pub struct Round {
     pub id: RoundId,
+    pub draft: Option<Draft>,
 }
 
 impl Round {
     pub fn reopen(&mut self) {
-        self.draft = None;
+        // The unsent draft stays with the round, so that an answer the reviewer typed before closing the pane is still there when it opens again.
     }
 }
";

pub(super) const DRAFTS: ChangedSource = ChangedSource {
    path: "src/drafts.rs",
    old: OLD,
    new: NEW,
    diff: DIFF,
};

pub(super) const CHANGE: FixedChange = FixedChange(&[DRAFTS]);
