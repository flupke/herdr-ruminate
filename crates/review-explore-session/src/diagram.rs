use review_explore::DiagramError;
use review_explore_page::CommandRefusal;

use crate::ExploreSession;

impl ExploreSession {
    pub(crate) fn diagram_failed(&mut self, error: DiagramError) -> Result<(), CommandRefusal> {
        self.save_from_page(|saved| {
            saved
                .exploration
                .record_diagram_error(error)
                .map_err(|error| error.to_string())
        })
    }
}
