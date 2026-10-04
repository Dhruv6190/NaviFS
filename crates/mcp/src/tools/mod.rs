//! Core MCP tools for NaviFS: search, inspect, open, and related.

pub mod inspect;
pub mod open;
pub mod related;
pub mod search;

pub use inspect::{EntitySummary, InspectArgs, InspectTool, MetadataSummary};
pub use open::{BoundedContentResponse, OpenArgs, OpenTool};
pub use related::{ConnectedEntity, OneHopGraphResponse, RelatedArgs, RelatedTool};
pub use search::{SearchArgs, SearchTool};
