use crate::{
    Action, ActionSearchRequest, AuthZenError, Resource, ResourceSearchRequest, SearchResponse,
    Subject, SubjectSearchRequest,
};

use super::AuthZenClient;

macro_rules! search_paginator {
    ($name:ident, $request:ty, $result:ty, $search:ident) => {
        /// Stateful Search pagination that keeps the initial query fixed and
        /// changes only the opaque continuation token between requests.
        #[must_use = "a paginator does not send a request until next_page is called"]
        pub struct $name {
            client: AuthZenClient,
            request: $request,
            finished: bool,
        }

        impl $name {
            fn new(client: AuthZenClient, request: $request) -> Self {
                Self {
                    client,
                    request,
                    finished: false,
                }
            }

            /// Returns the next page, or `None` after a response omits `page`
            /// or supplies an empty `next_token`.
            ///
            /// A failed request does not advance the token, so callers may
            /// retry by calling this method again.
            pub async fn next_page(
                &mut self,
            ) -> Result<Option<SearchResponse<$result>>, AuthZenError> {
                if self.finished {
                    return Ok(None);
                }
                let response = self.client.$search(self.request.clone()).await?;
                if let Some(token) = response.next_token() {
                    self.request = self.request.continuation(token);
                } else {
                    self.finished = true;
                }
                Ok(Some(response))
            }
        }
    };
}

search_paginator!(
    SubjectSearchPaginator,
    SubjectSearchRequest,
    Subject,
    search_subjects
);
search_paginator!(
    ResourceSearchPaginator,
    ResourceSearchRequest,
    Resource,
    search_resources
);
search_paginator!(
    ActionSearchPaginator,
    ActionSearchRequest,
    Action,
    search_actions
);

impl AuthZenClient {
    /// Starts stateful Subject Search pagination.
    ///
    /// Continuation requests preserve the validated initial request and
    /// replace only its opaque page token.
    pub fn paginate_subjects(
        &self,
        request: SubjectSearchRequest,
    ) -> Result<SubjectSearchPaginator, AuthZenError> {
        request.validate()?;
        Ok(SubjectSearchPaginator::new(self.clone(), request))
    }

    /// Starts stateful Resource Search pagination.
    ///
    /// Continuation requests preserve the validated initial request and
    /// replace only its opaque page token.
    pub fn paginate_resources(
        &self,
        request: ResourceSearchRequest,
    ) -> Result<ResourceSearchPaginator, AuthZenError> {
        request.validate()?;
        Ok(ResourceSearchPaginator::new(self.clone(), request))
    }

    /// Starts stateful Action Search pagination.
    ///
    /// Continuation requests preserve the validated initial request and
    /// replace only its opaque page token.
    pub fn paginate_actions(
        &self,
        request: ActionSearchRequest,
    ) -> Result<ActionSearchPaginator, AuthZenError> {
        request.validate()?;
        Ok(ActionSearchPaginator::new(self.clone(), request))
    }
}
