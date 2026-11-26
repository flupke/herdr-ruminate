use review_explore_page::{CommandRefusal, PageQuizResponse};

use crate::ExploreSession;

impl ExploreSession {
    pub(crate) fn quiz_from_page(
        &mut self,
        response: PageQuizResponse,
    ) -> Result<(), CommandRefusal> {
        let PageQuizResponse {
            conclusion,
            response,
        } = response;
        self.save_from_page(|saved| {
            saved
                .exploration
                .answer_quiz(&conclusion, response)
                .map_err(|error| error.to_string())
        })
    }
}
