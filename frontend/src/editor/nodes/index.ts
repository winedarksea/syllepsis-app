// Re-export all custom Lexical nodes for the editor config.

export { CategoryNode } from './CategoryNode';
export { ClozeNode } from './ClozeNode';

// Additional nodes (ReferenceNode, LocNode, CommentNode, TodoItemNode) extend
// from the same pattern — added as the dialect features become interactive.
// For Phase 1, @reference / loc: / %%comment%% tokens are preserved as plain text
// in the editor body and rendered correctly on read.
