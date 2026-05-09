# Pixors – UI & UX Guidelines

This document outlines the architecture, rules, and best practices for developing the user interface in the `pixors-desktop` crate.

## Architecture

The UI is built entirely using **Iced** (v0.14) and follows a strictly modular, React-like component architecture. The UI layer is completely decoupled from the application's business logic (`EditorState` and `pixors-state`).

### Directory Structure

```
pixors-desktop/src/
├── components/   # Reusable, atomic UI components (Button, Input, Slider, etc.)
├── layout/       # Structural layout wrappers (Sidebar, PaneGrid, Dialog)
├── panel/        # Complex, domain-specific panels (Layers, Filters)
├── page/         # High-level page assemblies
│   ├── mod.rs          # Exposes common page elements
│   ├── workspace_bar.rs
│   ├── status_bar.rs
│   ├── menu_bar.rs
│   ├── editor/         # The main editor page and its specific components
│   │   ├── mod.rs      # Page layout assembly
│   │   ├── toolbar.rs
│   │   ├── tab_bar.rs
│   │   └── viewport.rs
│   ├── darkroom/       # Darkroom page assembly
│   └── library/        # Library page assembly
└── modal/        # Overlay modals that block interaction with the underlying page
    ├── export/   # Export modal assembly
    └── ui_showcase.rs
```

## UX & Design Rules

### 1. Modals vs. Dialogs
- **Modals (`src/modal/`)**: Rendered *within* the Iced application window as an overlay over the main page content. They block interaction with the background until dismissed. Examples: Export, UI Showcase.
- **Dialogs (`src/dialog/` or via `rfd`)**: Native OS windows (e.g., File Open/Save dialogs). Do not confuse these with modals.

### 2. Standardized Components
Never construct raw Iced `button`, `text_input`, or `slider` widgets directly in your pages or panels. 
**Always** use the standardized components from `src/components/`. 
If a component needs a specific variation (e.g., a Ghost button or an Active state), extend the builder API in `src/components/` rather than styling it inline.

Example:
```rust
// ❌ WRONG: Inline styling
button(text("Cancel")).style(/* custom style */).on_press(...)

// ✅ RIGHT: Using the standardized builder
crate::components::button("Cancel")
    .variant(crate::components::ButtonVariant::Secondary)
    .on_press(...)
```

### 3. Builder Pattern
All custom components should implement a builder pattern to allow clear and extensible configurations without bloated function signatures.
```rust
crate::components::icon_button(crate::icons::PLUS)
    .size(16)
    .active(true)
    .on_press(Msg::Action)
```

### 4. Color & Theming
All colors, dimensions, and standard borders must be imported from `src/theme.rs`. Never hardcode colors (`Color::from_rgb(...)`) directly in components unless building a specialized color picker.

### 5. Page Encapsulation
- Components specific to a single page (e.g., `toolbar` is only used in `editor`) must reside within that page's module (`src/page/editor/toolbar.rs`).
- Components shared across multiple pages (e.g., `workspace_bar`) reside at the root of `src/page/`.
- Pages should only assemble the layout and route messages. They should not contain deep component rendering logic.

### 6. Event Handling
UI components should emit variants of the page's `Msg` enum. Avoid placing any business logic (state mutations, complex calculations) inside the UI layer. All interactions should dispatch an `Action` to `pixors-state` via the `Controller`.
