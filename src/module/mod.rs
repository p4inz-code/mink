//! Module system: multi-file compilation support (Session 34).
//!
//! MINK source files are modules. A `mod name;` declaration in a parent
//! module loads `name.mink` from the same directory. `use mod_name;` or
//! `use mod_name::Item;` imports public items from another module.
//! `pub` on declarations makes them visible across module boundaries.
//!
//! The module tree is built during source loading and threaded through
//! the compiler pipeline so that semantic analysis, type checking, HIR
//! lowering, MIR lowering, and backend compilation can resolve cross-
//! module references.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::source::SourceId;

/// Opaque identifier for a module within a compilation session.
///
/// Assigned sequentially as modules are discovered and loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(u32);

impl ModuleId {
    /// Creates a module id from its raw numeric value.
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw numeric value.
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModuleId({})", self.0)
    }
}

/// The visibility of a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// Private to the declaring module (default).
    Private,
    /// Visible to other modules (`pub`).
    Public,
}

impl Visibility {
    /// Whether this visibility is public.
    pub fn is_public(self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Metadata for a single module.
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// The module's unique identifier.
    pub id: ModuleId,
    /// The module's name (e.g. "shapes" for `shapes.mink`).
    pub name: String,
    /// The source file id this module was loaded from.
    pub source_id: SourceId,
    /// The file path this module was loaded from.
    pub path: PathBuf,
    /// The parent module, if any (None for the root module).
    pub parent: Option<ModuleId>,
    /// Child modules declared via `mod name;`.
    pub children: Vec<ModuleId>,
    /// Items declared in this module with their visibility.
    pub items: Vec<ModuleItem>,
    /// Maps item names to their index in the `items` vector.
    pub item_index: HashMap<String, usize>,
}

/// A declaration within a module, with its visibility.
#[derive(Debug, Clone)]
pub struct ModuleItem {
    /// The item's name.
    pub name: String,
    /// The item's visibility.
    pub visibility: Visibility,
    /// What kind of item this is.
    pub kind: ModuleItemKind,
}

/// The kind of a module-level item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleItemKind {
    /// A function declaration.
    Fn,
    /// A struct declaration.
    Struct,
    /// An enum declaration.
    Enum,
    /// A `let` binding.
    Let,
    /// A `const` binding.
    Const,
}

impl fmt::Display for ModuleItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fn => write!(f, "function"),
            Self::Struct => write!(f, "struct"),
            Self::Enum => write!(f, "enum"),
            Self::Let => write!(f, "let binding"),
            Self::Const => write!(f, "const binding"),
        }
    }
}

/// A tree of modules for a compilation session.
///
/// The root module is the entry-point file; `mod name;` declarations
/// create child modules loaded from the filesystem.
#[derive(Debug, Clone)]
pub struct ModuleTree {
    /// All modules, indexed by `ModuleId::raw()`.
    modules: Vec<ModuleInfo>,
    /// Maps module names (relative to their parent) to their ids.
    name_index: HashMap<String, ModuleId>,
    /// The root module.
    root: ModuleId,
}

impl ModuleTree {
    /// Creates a new empty module tree with a root module.
    pub fn new(root_name: String, root_source: SourceId, root_path: PathBuf) -> Self {
        let root_id = ModuleId(0);
        let root = ModuleInfo {
            id: root_id,
            name: root_name,
            source_id: root_source,
            path: root_path,
            parent: None,
            children: Vec::new(),
            items: Vec::new(),
            item_index: HashMap::new(),
        };
        let mut name_index = HashMap::new();
        name_index.insert(root.name.clone(), root_id);
        Self {
            modules: vec![root],
            name_index,
            root: root_id,
        }
    }

    /// The root module id.
    pub fn root(&self) -> ModuleId {
        self.root
    }

    /// The root module info.
    pub fn root_info(&self) -> &ModuleInfo {
        &self.modules[self.root.0 as usize]
    }

    /// Returns the module info for `id`.
    pub fn get(&self, id: ModuleId) -> Option<&ModuleInfo> {
        self.modules.get(id.0 as usize)
    }

    /// Returns the module info for `id`, mutable.
    pub fn get_mut(&mut self, id: ModuleId) -> Option<&mut ModuleInfo> {
        self.modules.get_mut(id.0 as usize)
    }

    /// All modules in discovery order.
    pub fn modules(&self) -> &[ModuleInfo] {
        &self.modules
    }

    /// Number of modules.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether the tree has no modules.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Registers a new module and returns its id.
    pub fn add_module(
        &mut self,
        name: String,
        source_id: SourceId,
        path: PathBuf,
        parent: Option<ModuleId>,
    ) -> ModuleId {
        let id = ModuleId(self.modules.len() as u32);
        let module = ModuleInfo {
            id,
            name: name.clone(),
            source_id,
            path,
            parent,
            children: Vec::new(),
            items: Vec::new(),
            item_index: HashMap::new(),
        };
        self.modules.push(module);
        self.name_index.insert(name, id);

        // Register as child of parent.
        if let Some(parent_id) = parent {
            if let Some(parent_info) = self.modules.get_mut(parent_id.0 as usize) {
                parent_info.children.push(id);
            }
        }

        id
    }

    /// Looks up a module by name (within a parent module's children).
    pub fn lookup_child(&self, parent: ModuleId, name: &str) -> Option<ModuleId> {
        let parent_info = self.modules.get(parent.0 as usize)?;
        for &child_id in &parent_info.children {
            if let Some(child_info) = self.modules.get(child_id.0 as usize) {
                if child_info.name == name {
                    return Some(child_id);
                }
            }
        }
        None
    }

    /// Recursively looks up a module by path segments starting from a root.
    pub fn lookup_path(&self, root: ModuleId, segments: &[String]) -> Option<ModuleId> {
        let mut current = root;
        for segment in segments {
            current = self.lookup_child(current, segment)?;
        }
        Some(current)
    }

    /// Returns the module id for a given source file.
    pub fn find_by_source(&self, source_id: SourceId) -> Option<ModuleId> {
        self.modules
            .iter()
            .find(|m| m.source_id == source_id)
            .map(|m| m.id)
    }

    /// Discovers the file path for a `mod name;` declaration relative to
    /// the parent module's directory.
    pub fn module_file_path(parent_dir: &Path, name: &str) -> PathBuf {
        parent_dir.join(format!("{name}.mink"))
    }

    /// Registers items for a module from its parsed AST.
    pub fn register_items(&mut self, module_id: ModuleId, items: Vec<ModuleItem>) {
        if let Some(module) = self.modules.get_mut(module_id.0 as usize) {
            for (i, item) in items.iter().enumerate() {
                module.item_index.insert(item.name.clone(), i);
            }
            module.items = items;
        }
    }

    /// Checks whether a name exists as a public item in the given module.
    pub fn has_public_item(&self, module_id: ModuleId, name: &str) -> bool {
        let Some(module) = self.modules.get(module_id.0 as usize) else {
            return false;
        };
        module
            .item_index
            .get(name)
            .and_then(|&idx| module.items.get(idx))
            .map(|item| item.visibility.is_public())
            .unwrap_or(false)
    }
}

/// An import brought in by a `use` declaration.
#[derive(Debug, Clone)]
pub struct ImportEntry {
    /// The name the item is imported as (last segment of the use path,
    /// or an alias).
    pub alias: String,
    /// The source module id.
    pub module_id: ModuleId,
    /// The original name in the source module.
    pub original_name: String,
    /// The kind of imported item.
    pub kind: ModuleItemKind,
    /// Whether the import is `pub use` (re-export).
    pub re_export: bool,
}

/// A cross-module symbol registry: maps module names to their public
/// items, enabling cross-module symbol resolution.
#[derive(Debug, Clone, Default)]
pub struct ModuleRegistry {
    /// Module name → list of (item_name, visibility, kind).
    modules: HashMap<String, Vec<RegisteredItem>>,
}

/// An item registered in the cross-module registry.
#[derive(Debug, Clone)]
pub struct RegisteredItem {
    /// The item's name.
    pub name: String,
    /// The item's visibility.
    pub visibility: Visibility,
    /// The kind of item.
    pub kind: ModuleItemKind,
}

impl ModuleRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers public items for a module.
    pub fn register_module(&mut self, module_name: String, items: Vec<RegisteredItem>) {
        self.modules.insert(module_name, items);
    }

    /// Looks up an item in a module by name.
    pub fn lookup(&self, module_name: &str, item_name: &str) -> Option<&RegisteredItem> {
        let items = self.modules.get(module_name)?;
        items.iter().find(|i| i.name == item_name)
    }

    /// Checks if a module exists in the registry.
    pub fn has_module(&self, module_name: &str) -> bool {
        self.modules.contains_key(module_name)
    }

    /// Returns all items for a module.
    pub fn module_items(&self, module_name: &str) -> Option<&[RegisteredItem]> {
        self.modules.get(module_name).map(|v| v.as_slice())
    }

    /// All registered module names.
    pub fn modules(&self) -> impl Iterator<Item = &str> {
        self.modules.keys().map(|s| s.as_str())
    }
}

/// Resolved imports for a module: maps local names to their source.
#[derive(Debug, Clone, Default)]
pub struct ImportTable {
    /// Local name → import entry.
    entries: HashMap<String, ImportEntry>,
}

impl ImportTable {
    /// Creates an empty import table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an import. Returns false if the name already exists.
    pub fn insert(&mut self, entry: ImportEntry) -> bool {
        if self.entries.contains_key(&entry.alias) {
            return false;
        }
        self.entries.insert(entry.alias.clone(), entry);
        true
    }

    /// Looks up an imported name.
    pub fn lookup(&self, name: &str) -> Option<&ImportEntry> {
        self.entries.get(name)
    }

    /// Whether a name is imported.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// All import entries.
    pub fn entries(&self) -> &HashMap<String, ImportEntry> {
        &self.entries
    }
}
