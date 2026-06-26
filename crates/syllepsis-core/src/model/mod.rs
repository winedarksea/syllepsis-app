//! Domain object model: note/category/world types and the full metadata schema.
//!
//! Re-exports the commonly used types so callers write `model::Note` rather than
//! `model::note::Note`.

pub mod category;
pub mod classification;
pub mod commentary;
pub mod metadata;
pub mod note;
pub mod object_type;
pub mod prior;
pub mod style_card;
pub mod world;

pub use category::Category;
pub use classification::{Basis, Checkability, Classification, Priority, Stability, StatementType};
pub use commentary::{
    CommentaryKind, CommentaryMetadata, CommentarySource, CommentaryStatus, CommentaryTargetField,
};
pub use metadata::{
    Authorship, DateMetadata, FlexDate, ForkInfo, Kanban, Lifecycle, LockMode, Metadata,
    NoteStatus, NoteVisibility, PackMembership,
};
pub use note::{AssetMetadata, Note, SummaryWarning};
pub use object_type::ObjectType;
pub use prior::{PriorEdge, PriorKind, PriorRef};
pub use style_card::StyleCard;
pub use world::{SpatialRegion, World, WorldKind, DEFAULT_WORLD_ID};
