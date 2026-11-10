//! Reuse the original resource/class producer; never accept a completed UI tree.

use super::{Document, Error, Session, TreeState, UiState, ViewState, invalid};
use crate::printing::{
    self,
    native::{Factory, Resources},
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct NativeView {
    pub factory: Factory,
    pub base_bc: u8,
    /// Base/Common/Groups are concrete factories without a print dataset.
    pub dataset: Option<printing::ViewState>,
}

#[derive(Debug, Clone)]
pub struct NativeState {
    asset: Arc<[u8]>,
    pub show_split: u32,
    pub factories: Vec<Factory>,
    pub active: Option<NativeView>,
}

impl NativeState {
    /// Share the same original asset with the menu/resource owner by borrowing.
    pub fn resources(&self) -> std::io::Result<Resources<'_>> {
        Resources::from_pe(&self.asset)
    }
}

fn source_error(_: std::io::Error) -> Error {
    invalid("native lifecycle resource/class producer failed")
}

impl UiState {
    /// asset comes from printing::native::read_source or an equivalent explicit
    /// original asset. No second PE parser, field codec or client-built tree.
    /// Menu handlers remain fail-closed until their OWN producer is registered.
    pub fn from_native_source(
        asset: Arc<[u8]>,
        document: &mut Document,
        session: &mut Session,
        show_split: u32,
    ) -> Result<Self, Error> {
        let mut ui = Self::headless();
        ui.native = Some(NativeState {
            asset,
            show_split,
            factories: Vec::new(),
            active: None,
        });
        ui.configuration = Some(session.context.fields());
        ui.rebuild_native(document)?;
        Ok(ui)
    }

    pub(super) fn rebuild_native(&mut self, document: &mut Document) -> Result<(), Error> {
        let native = self
            .native
            .as_mut()
            .ok_or(invalid("native lifecycle source is absent"))?;
        let count = u32::from_le_bytes(
            document
                .current
                .get(0x3fb40..0x3fb44)
                .ok_or(invalid("normal group count source unavailable"))?
                .try_into()
                .unwrap(),
        ) as usize;
        let resources = native.resources().map_err(source_error)?;
        let factories = resources.tree(count).map_err(source_error)?;
        let mut tree = TreeState::default();
        let mut views = Vec::new();
        for (ordinal, factory) in factories.iter().enumerate() {
            // Internal identity is distinct from a native caption/title string.
            let mut key = format!(
                "{ordinal}:{:?}:{},{}:",
                factory.class, factory.resource, factory.node
            )
            .into_bytes();
            key.extend_from_slice(&factory.parameter);
            tree.keys.push(key.clone());
            views.push(ViewState {
                key,
                views: Vec::new(),
            });
        }
        native.factories = factories;
        native.active = None;
        self.view_sources = views;
        self.tree = Some(tree);
        Ok(())
    }

    /// A selected original node is a gesture, not a constructed view/outcome.
    pub fn select_native(
        &mut self,
        node: i32,
        document: &mut Document,
        session: &mut Session,
    ) -> Result<(), Error> {
        let native = self
            .native
            .as_ref()
            .ok_or(invalid("native lifecycle source is absent"))?;
        let mut matches = native
            .factories
            .iter()
            .enumerate()
            .filter(|(_, factory)| factory.node == node);
        let index = matches
            .next()
            .map(|(index, _)| index)
            .ok_or(invalid("selected node has no original registered factory"))?;
        if matches.next().is_some() {
            return Err(invalid(
                "native node has multiple tree bindings; select its source ordinal",
            ));
        }
        self.select_native_index(index, document, session)
    }

    /// Distinct original tree items may refer to the same factory/node.
    pub fn select_native_index(
        &mut self,
        index: usize,
        document: &mut Document,
        session: &mut Session,
    ) -> Result<(), Error> {
        let native = self
            .native
            .as_ref()
            .ok_or(invalid("native lifecycle source is absent"))?;
        if index >= native.factories.len() {
            return Err(invalid("native tree source ordinal outside registration"));
        }
        self.destroy_view(document);
        self.tree
            .as_mut()
            .ok_or(invalid("native tree is absent"))?
            .selected = Some(index);
        self.sync25(document, session)
    }

    pub(super) fn instantiate_native(&mut self, document: &mut Document) -> Result<(), Error> {
        let Some(native) = &mut self.native else {
            return Ok(());
        };
        let tree = self.tree.as_ref().ok_or(invalid("native tree is absent"))?;
        let Some(index) = tree.selected else {
            return Ok(());
        };
        let factory = native
            .factories
            .get(index)
            .ok_or(invalid("native selection outside factory registration"))?
            .clone();
        let base_bc = u8::from(
            native
                .resources()
                .map_err(source_error)?
                .no_print(&factory)
                .map_err(source_error)?,
        );
        let mut dataset = factory
            .view(&document.current, native.show_split)
            .map_err(source_error)?;
        if let Some(view) = &mut dataset {
            view.bc = (view.bc & !1) | base_bc;
        }
        native.active = Some(NativeView {
            factory,
            base_bc,
            dataset,
        });
        Ok(())
    }
}
