//! One shallow tree of sections, editor pages and real row groups.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Section {
    Memories,
    Digital,
    General,
    Radio,
    Connections,
}
impl Section {
    pub const ALL: [Self; 5] = [
        Self::Memories,
        Self::Digital,
        Self::General,
        Self::Radio,
        Self::Connections,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Memories => "Memories",
            Self::Digital => "Digital",
            Self::General => "General",
            Self::Radio => "Radio",
            Self::Connections => "Connections",
        }
    }
    pub fn categories(self) -> &'static [Category] {
        match self {
            Self::Memories => &[Category::Memory, Category::Repeater, Category::GpsMemory],
            Self::Digital => &[Category::Ur, Category::Calls, Category::Dv, Category::Gps],
            Self::General => &[
                Category::Display,
                Category::Sound,
                Category::Function,
                Category::Scan,
            ],
            Self::Radio => &[Category::Rf, Category::Cw, Category::Rtty, Category::Scope],
            Self::Connections => &[
                Category::Connections,
                Category::Network,
                Category::Bluetooth,
            ],
        }
    }
    fn for_category(category: Category) -> Self {
        Self::ALL
            .into_iter()
            .find(|s| s.categories().contains(&category))
            .expect("every editor page belongs to a section")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum NodeId {
    Section(Section),
    Category(Category),
    Group(Category, usize),
}
impl NodeId {
    pub fn branch(self) -> bool {
        matches!(
            self,
            Self::Section(_)
                | Self::Category(Category::Memory | Category::Repeater | Category::GpsMemory)
        )
    }
}
pub(super) struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub depth: usize,
    pub label: String,
}

impl App {
    pub(super) fn nodes(&self) -> Vec<Node> {
        let mut nodes = Vec::new();
        for section in Section::ALL {
            let parent = NodeId::Section(section);
            nodes.push(Node {
                id: parent,
                parent: None,
                depth: 0,
                label: section.title().into(),
            });
            if !self.expanded.contains(&parent) {
                continue;
            }
            for &category in section.categories() {
                let id = NodeId::Category(category);
                nodes.push(Node {
                    id,
                    parent: Some(parent),
                    depth: 1,
                    label: category.title().into(),
                });
                if !id.branch() || !self.expanded.contains(&id) {
                    continue;
                }
                let Some(editor) = &self.editor else {
                    continue;
                };
                match catalog::groups(editor, category) {
                    Ok(groups) => {
                        for group in groups {
                            nodes.push(Node {
                                id: NodeId::Group(category, group.index),
                                parent: Some(id),
                                depth: 2,
                                label: format!("{:02} {}", group.index, group.name)
                                    .trim_end()
                                    .into(),
                            });
                        }
                    }
                    Err(_) => nodes
                        .last_mut()
                        .expect("category was just inserted")
                        .label
                        .push_str(" (unavailable)"),
                }
            }
        }
        nodes
    }
    pub(super) fn select_node(&mut self) {
        let nodes = self.nodes();
        let Some(node) = nodes.get(self.navigation) else {
            return;
        };
        self.section = None;
        self.row_group = None;
        match node.id {
            NodeId::Section(section) => self.section = Some(section),
            NodeId::Category(category) => self.category = category,
            NodeId::Group(Category::Memory, group) => {
                self.category = Category::Memory;
                self.group = group;
            }
            NodeId::Group(category, group) => {
                self.category = category;
                self.row_group = Some(group);
            }
        }
        self.row = 0;
        self.filter.clear();
        self.rows = TableState::default();
        self.refresh();
    }
    pub(super) fn open_node(&mut self, id: NodeId) {
        match id {
            NodeId::Section(_) => {}
            NodeId::Category(category) => {
                self.expanded
                    .insert(NodeId::Section(Section::for_category(category)));
            }
            NodeId::Group(category, _) => {
                self.expanded
                    .insert(NodeId::Section(Section::for_category(category)));
                self.expanded.insert(NodeId::Category(category));
            }
        }
        if let Some(index) = self.nodes().iter().position(|n| n.id == id) {
            self.navigation = index;
            self.select_node();
        }
    }
    pub(super) fn sync_navigation(&mut self) {
        let id = if let Some(section) = self.section {
            NodeId::Section(section)
        } else if self.category == Category::Memory {
            NodeId::Group(self.category, self.group)
        } else if let Some(group) = self.row_group {
            NodeId::Group(self.category, group)
        } else {
            NodeId::Category(self.category)
        };
        let nodes = self.nodes();
        self.navigation = nodes
            .iter()
            .position(|node| node.id == id)
            .or_else(|| {
                nodes
                    .iter()
                    .position(|node| node.id == NodeId::Category(self.category))
            })
            .unwrap_or(self.navigation.min(nodes.len().saturating_sub(1)));
    }
    pub(super) fn tree_key(&mut self, code: KeyCode) {
        let nodes = self.nodes();
        let Some(node) = nodes.get(self.navigation) else {
            return;
        };
        let id = node.id;
        match code {
            KeyCode::Left => {
                if self.expanded.remove(&id) {
                    return;
                }
                if let Some(parent) = node.parent {
                    self.open_node(parent);
                }
            }
            KeyCode::Right => {
                if id.branch() {
                    if self.expanded.insert(id) {
                        return;
                    }
                    if let Some(child) = nodes
                        .get(self.navigation + 1)
                        .filter(|n| n.parent == Some(id))
                    {
                        self.open_node(child.id);
                    }
                } else {
                    self.pane = Pane::Items;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if id.branch() {
                    if !self.expanded.remove(&id) {
                        self.expanded.insert(id);
                    }
                } else {
                    self.pane = Pane::Items;
                }
            }
            _ => {}
        }
    }
}
