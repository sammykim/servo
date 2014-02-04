/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A safe wrapper for DOM nodes that prevents layout from mutating the DOM, from letting DOM nodes
//! escape, and from generally doing anything that it isn't supposed to. This is accomplished via
//! a simple whitelist of allowed operations.
//!
//! As a security wrapper is only as good as its whitelist, be careful when adding operations to
//! this list. The cardinal rules are:
//!
//! (1) Layout is not allowed to mutate the DOM.
//!
//! (2) Layout is not allowed to see anything with `Abstract` in the name, because it could hang
//!     onto these objects and cause use-after-free.

use extra::url::Url;
use script::dom::element::{Element, HTMLAreaElementTypeId, HTMLAnchorElementTypeId};
use script::dom::element::{HTMLLinkElementTypeId};
use script::dom::htmliframeelement::HTMLIFrameElement;
use script::dom::htmlimageelement::HTMLImageElement;
use script::dom::namespace::Namespace;
use script::dom::node::{AbstractNode, DocumentNodeTypeId, ElementNodeTypeId, Node, NodeTypeId};
use script::dom::text::Text;
use servo_msg::constellation_msg::{PipelineId, SubpageId};
use std::cast;
use style::{PropertyDeclarationBlock, TElement, TNode};
use style::{PseudoElement, Before, After};
use style::computed_values::display;
use layout::util::LayoutDataAccess;

/// A wrapper so that layout can access only the methods that it should have access to. Layout must
/// only ever see these and must never see instances of `AbstractNode`.
#[deriving(Clone, Eq)]
pub struct LayoutNode<'a> {
    /// The wrapped node.
    priv node: AbstractNode,

    /// Being chained to a value prevents `LayoutNode`s from escaping.
    priv chain: &'a (),
}

impl<'ln> LayoutNode<'ln> {
    /// Creates a new layout node, scoped to the given closure.
    pub unsafe fn with_layout_node<R>(node: AbstractNode, f: <'a> |LayoutNode<'a>| -> R) -> R {
        let heavy_iron_ball = ();
        f(LayoutNode {
            node: node,
            chain: &heavy_iron_ball,
        })
    }

    /// Creates a new layout node with the same lifetime as this layout node.
    pub unsafe fn new_with_this_lifetime(&self, node: AbstractNode) -> LayoutNode<'ln> {
        LayoutNode {
            node: node,
            chain: self.chain,
        }
    }

    pub fn set_parent_node(&mut self, new_parent_node: &LayoutNode) {
        self.node.mut_node().parent_node = Some(new_parent_node.node);
    }

    pub fn set_first_child(&mut self, new_first_child: &LayoutNode) {
        self.node.mut_node().first_child = Some(new_first_child.node);
    }

    pub fn set_last_child(&mut self, new_last_child: &LayoutNode) {
        self.node.mut_node().last_child = Some(new_last_child.node);
    }

    pub fn set_prev_sibling(&mut self, new_prev_sibling: &LayoutNode) {
        self.node.mut_node().prev_sibling = Some(new_prev_sibling.node);
    }

    pub fn set_next_sibling(&mut self, new_next_sibling: &LayoutNode) {
        self.node.mut_node().next_sibling = Some(new_next_sibling.node);
    }

    fn get_pseudo_node(&self, pseudo_element: PseudoElement) -> Option<LayoutNode<'ln>> {
        macro_rules! get_pseudo_node(
                ($pseudo_parent_node: ident, $pseudo_node: ident) => {
                    if self.is_text() {
                        let layout_data_ref = self.borrow_layout_data();
                        return layout_data_ref.get().as_ref().and_then(|ldw|{
                            ldw.data.$pseudo_parent_node.as_ref().and_then(|$pseudo_parent_node|{
                                if $pseudo_parent_node.get_display() == display::inline {
                                    ldw.data.$pseudo_node.as_ref().and_then(|$pseudo_node|{
                                        unsafe{
                                            Some(self.new_with_this_lifetime($pseudo_node.node))
                                        }
                                    })
                                } else {
                                    None
                                }
                            })
                        });
                    } else if self.is_element() {
                        match self.first_child() {
                            Some(first_child) => {
                                let layout_data_ref = first_child.borrow_layout_data();
                                return layout_data_ref.get().as_ref().and_then(|ldw|{
                                    ldw.data.$pseudo_parent_node.as_ref().and_then(|$pseudo_parent_node|{
                                        if $pseudo_parent_node.get_display() == display::block {
                                            ldw.data.$pseudo_parent_node.as_ref().and_then(|$pseudo_parent_node|{
                                                unsafe{
                                                    Some(self.new_with_this_lifetime($pseudo_parent_node.node))
                                                }
                                            })
                                        } else {
                                            None
                                        }
                                    })
                                });
                            }
                            None => {
                                return None
                            }
                        }
                    } else {
                        return None
                    }
                }
        )
        if pseudo_element == Before {
            return get_pseudo_node!(before_parent_node, before_node)
        } else if pseudo_element == After {
            return get_pseudo_node!(after_parent_node, after_node)
        } else {
            return None
        }
    }

    /// Returns the interior of this node as a `Node`. This is highly unsafe for layout to call
    /// and as such is marked `unsafe`.
    pub unsafe fn get<'a>(&'a self) -> &'a Node {
        cast::transmute(self.node.node())
    }

    /// Returns the interior of this node as an `AbstractNode`. This is highly unsafe for layout to
    /// call and as such is marked `unsafe`.
    pub unsafe fn get_abstract(&self) -> AbstractNode {
        self.node
    }

    /// Returns the first child of this node.
    pub fn first_child(&self) -> Option<LayoutNode<'ln>> {
        let first_child = unsafe {
                              self.node.first_child().map(|node| self.new_with_this_lifetime(node)) 
                          };

        if first_child.is_some() {
            match first_child { 
                Some(first_child) if first_child.is_text() => {
                    let before_node = first_child.get_pseudo_node(Before);
                    if before_node.is_some() {
                        return before_node
                    }
                }
                _ => ()
            }
        }

        return first_child
    }

    pub fn next_pseudo_sibling(&self) -> Option<LayoutNode<'ln>> {
        unsafe {
            self.node.node().next_sibling.map(|node| self.new_with_this_lifetime(node)) 
        }
    }

    /// Iterates over this node and all its descendants, in preorder.
    ///
    /// FIXME(pcwalton): Terribly inefficient. We should use parallelism.
    pub fn traverse_preorder(&self) -> LayoutTreeIterator<'ln> {
        let mut nodes = ~[];
        gather_layout_nodes(self, &mut nodes, false);
        LayoutTreeIterator::new(nodes)
    }

    /// Returns an iterator over this node's children.
    pub fn children(&self) -> LayoutNodeChildrenIterator<'ln> {
        LayoutNodeChildrenIterator {
            current_node: self.first_child(),
        }
    }

    /// Returns the type ID of this node. Fails if this node is borrowed mutably.
    pub fn type_id(&self) -> NodeTypeId {
        self.node.type_id()
    }

    /// If this is an image element, returns its URL. If this is not an image element, fails.
    ///
    /// FIXME(pcwalton): Don't copy URLs.
    pub fn image_url(&self) -> Option<Url> {
        unsafe {
            self.with_image_element(|image_element| {
                image_element.image.as_ref().map(|url| (*url).clone())
            })
        }
    }

    /// Downcasts this node to an image element and calls the given closure.
    ///
    /// FIXME(pcwalton): RAII.
    unsafe fn with_image_element<R>(self, f: |&HTMLImageElement| -> R) -> R {
        if !self.node.is_image_element() {
            fail!(~"node is not an image element");
        }
        self.node.transmute(f)
    }

    /// If this node is an iframe element, returns its pipeline and subpage IDs. If this node is
    /// not an iframe element, fails.
    pub fn iframe_pipeline_and_subpage_ids(&self) -> (PipelineId, SubpageId) {
        unsafe {
            self.with_iframe_element(|iframe_element| {
                let size = iframe_element.size.unwrap();
                (size.pipeline_id, size.subpage_id)
            })
        }
    }

    /// Downcasts this node to an iframe element and calls the given closure.
    ///
    /// FIXME(pcwalton): RAII.
    unsafe fn with_iframe_element<R>(self, f: |&HTMLIFrameElement| -> R) -> R {
        if !self.node.is_iframe_element() {
            fail!(~"node is not an iframe element");
        }
        self.node.transmute(f)
    }

    /// Returns true if this node is a text node or false otherwise.
    #[inline]
    pub fn is_text(self) -> bool {
        self.node.is_text()
    }

    /// Returns true if this node consists entirely of ignorable whitespace and false otherwise.
    /// Ignorable whitespace is defined as whitespace that would be removed per CSS 2.1 § 16.6.1.
    pub fn is_ignorable_whitespace(&self) -> bool {
        unsafe {
            self.is_text() && self.with_text(|text| text.element.data.is_whitespace())
        }
    }

    /// If this is a text node, copies out the text. If this is not a text node, fails.
    ///
    /// FIXME(pcwalton): Don't copy text. Atomically reference count instead.
    pub fn text(&self) -> ~str {
        unsafe {
            self.with_text(|text| text.element.data.to_str())
        }
    }

    /// Downcasts this node to a text node and calls the given closure.
    ///
    /// FIXME(pcwalton): RAII.
    unsafe fn with_text<R>(self, f: |&Text| -> R) -> R {
        self.node.with_imm_text(f)
    }

    /// Dumps this node tree, for debugging.
    pub fn dump(&self) {
        self.node.dump()
    }

    /// Returns a string that describes this node, for debugging.
    pub fn debug_str(&self) -> ~str {
        self.node.debug_str()
    }

    pub fn necessary_pseudo_elements(&self) -> ~[PseudoElement] {
        let mut pseudo_elements = ~[];

        let ldw = self.borrow_layout_data();
        let ldw_ref = ldw.get().get_ref();
        if self.parent_node().is_none() {
            return ~[];
        }
        let p = self.parent_node().unwrap();
        let p_ldw = p.borrow_layout_data();
        let p_ldw_ref = p_ldw.get().get_ref();

        if p_ldw_ref.data.before_style.is_some() && ldw_ref.data.before_node.is_none() {
            pseudo_elements.push(Before);
        }
        if p_ldw_ref.data.after_style.is_some() && ldw_ref.data.after_node.is_none() {
            pseudo_elements.push(After);
        }
 
        return pseudo_elements
    }

    /// Traverses the tree in postorder.
    ///
    /// TODO(pcwalton): Offer a parallel version with a compatible API.
    pub fn traverse_postorder<T:PostorderNodeTraversal>(self, traversal: &T) -> bool {
        if traversal.should_prune(self) {
            return true
        }

        let mut opt_kid = self.first_child();
        loop {
            match opt_kid {
                None => break,
                Some(kid) => {
                    if !kid.traverse_postorder(traversal) {
                        return false
                    }
                    opt_kid = kid.next_sibling()
                }
            }
        }

        traversal.process(self)
    }

    /// Traverses the tree in postorder.
    ///
    /// TODO(pcwalton): Offer a parallel version with a compatible API.
    pub fn traverse_postorder_mut<T:PostorderNodeMutTraversal>(mut self, traversal: &mut T)
                                  -> bool {
        if traversal.should_prune(self) {
            return true
        }

        let mut opt_kid = self.first_child();
        loop {
            match opt_kid {
                None => break,
                Some(kid) => {
                    if !kid.traverse_postorder_mut(traversal) {
                        return false
                    }
                    opt_kid = kid.next_sibling()
                }
            }
        }

        traversal.process(self)
    }
}

impl<'ln> TNode<LayoutElement<'ln>> for LayoutNode<'ln> {
    fn parent_node(&self) -> Option<LayoutNode<'ln>> {
        unsafe {
            self.node.node().parent_node.map(|node| self.new_with_this_lifetime(node))
        }
    }

    fn prev_sibling(&self) -> Option<LayoutNode<'ln>> {
        if self.is_element() && self.node.with_imm_element(|element| "after" == element.tag_name) || 
           (self.is_text() && self.parent_node().unwrap().node.with_imm_element(|element| "after" == element.tag_name)) {
            return unsafe { 
                       self.node.node().prev_sibling.map(|node| self.new_with_this_lifetime(node))
                   }
        }

        let before_layout_node = self.get_pseudo_node(After);
        if before_layout_node.is_some() {
            return before_layout_node
        }

        let prev_sibling = unsafe{
                               self.node.node().prev_sibling.map(|node| self.new_with_this_lifetime(node))
                           };

        prev_sibling.map(|prev_sibling| prev_sibling.get_pseudo_node(After).or_else(|| Some(prev_sibling)).unwrap()) 
    }

    fn next_sibling(&self) -> Option<LayoutNode<'ln>> {
        if (self.is_element() && self.node.with_imm_element(|element| element.tag_name == ~"before"))
            || (self.is_text() && self.parent_node().unwrap().node.with_imm_element(|element| element.tag_name == ~"before")) {
            return unsafe{ self.node.node().next_sibling.map(|node| self.new_with_this_lifetime(node)) }
        }

        let after_layout_node = self.get_pseudo_node(After);
        if after_layout_node.is_some() { return after_layout_node }

        let next_sibling = unsafe{ self.node.node().next_sibling.map(|node| self.new_with_this_lifetime(node)) };

        next_sibling.map(|next_sibling| next_sibling.get_pseudo_node(Before).or_else(|| Some(next_sibling)).unwrap())
    }

    fn is_element(&self) -> bool {
        match self.node.type_id() {
            ElementNodeTypeId(..) => true,
            _ => false
        }
    }

    fn is_document(&self) -> bool {
        match self.node.type_id() {
            DocumentNodeTypeId(..) => true,
            _ => false
        }
    }

    /// If this is an element, accesses the element data. Fails if this is not an element node.
    #[inline]
    fn with_element<R>(&self, f: |&LayoutElement<'ln>| -> R) -> R {
        self.node.with_imm_element(|element| {
            // FIXME(pcwalton): Workaround until Rust gets multiple lifetime parameters on
            // implementations.
            unsafe {
                f(&LayoutElement {
                    element: cast::transmute_region(element),
                })
            }
        })
    }
}

pub struct LayoutNodeChildrenIterator<'a> {
    priv current_node: Option<LayoutNode<'a>>,
}

impl<'a> Iterator<LayoutNode<'a>> for LayoutNodeChildrenIterator<'a> {
    fn next(&mut self) -> Option<LayoutNode<'a>> {
        let node = self.current_node;
        self.current_node = self.current_node.and_then(|node| {
            node.next_sibling()
        });
        node
    }
}

pub struct LayoutPseudoNode {
    /// The wrapped node.
    priv node: AbstractNode,
    priv display: display::T
}

impl LayoutPseudoNode {
    pub fn from_layout_pseudo(node: AbstractNode, display: display::T) -> LayoutPseudoNode {
        LayoutPseudoNode {
            node: node,
            display: display
        }
    }

    fn get_display(&self) -> display::T {
        self.display
    }
}

impl Drop for LayoutPseudoNode {
    fn drop(&mut self) {
        if self.node.is_element() {
            let _: ~Element = unsafe { cast::transmute(self.node) };
        } else if self.node.is_text() {
            let _: ~Text = unsafe { cast::transmute(self.node) };
        }
    }
}

// FIXME: Do this without precomputing a vector of refs.
// Easy for preorder; harder for postorder.
//
// FIXME(pcwalton): Parallelism! Eventually this should just be nuked.
pub struct LayoutTreeIterator<'a> {
    priv nodes: ~[LayoutNode<'a>],
    priv index: uint,
}

impl<'a> LayoutTreeIterator<'a> {
    fn new(nodes: ~[LayoutNode<'a>]) -> LayoutTreeIterator<'a> {
        LayoutTreeIterator {
            nodes: nodes,
            index: 0,
        }
    }
}

impl<'a> Iterator<LayoutNode<'a>> for LayoutTreeIterator<'a> {
    fn next(&mut self) -> Option<LayoutNode<'a>> {
        if self.index >= self.nodes.len() {
            None
        } else {
            let v = self.nodes[self.index].clone();
            self.index += 1;
            Some(v)
        }
    }
}

/// FIXME(pcwalton): This is super inefficient.
fn gather_layout_nodes<'a>(cur: &LayoutNode<'a>, refs: &mut ~[LayoutNode<'a>], postorder: bool) {
    if !postorder {
        refs.push(cur.clone());
    }
    for kid in cur.children() {
        gather_layout_nodes(&kid, refs, postorder)
    }
    if postorder {
        refs.push(cur.clone());
    }
}

/// A bottom-up, parallelizable traversal.
pub trait PostorderNodeTraversal {
    /// The operation to perform. Return true to continue or false to stop.
    fn process<'a>(&'a self, node: LayoutNode<'a>) -> bool;

    /// Returns true if this node should be pruned. If this returns true, we skip the operation
    /// entirely and do not process any descendant nodes. This is called *before* child nodes are
    /// visited. The default implementation never prunes any nodes.
    fn should_prune<'a>(&'a self, _node: LayoutNode<'a>) -> bool {
        false
    }
}

/// A bottom-up, parallelizable traversal.
pub trait PostorderNodeMutTraversal {
    /// The operation to perform. Return true to continue or false to stop.
    fn process<'a>(&'a mut self, node: LayoutNode<'a>) -> bool;

    /// Returns true if this node should be pruned. If this returns true, we skip the operation
    /// entirely and do not process any descendant nodes. This is called *before* child nodes are
    /// visited. The default implementation never prunes any nodes.
    fn should_prune<'a>(&'a self, _node: LayoutNode<'a>) -> bool {
        false
    }
}

/// A wrapper around elements that ensures layout can only ever access safe properties.
pub struct LayoutElement<'le> {
    priv element: &'le Element,
}

impl<'le> LayoutElement<'le> {
    pub fn style_attribute(&self) -> &'le Option<PropertyDeclarationBlock> {
        &self.element.style_attribute
    }
}

impl<'le> TElement for LayoutElement<'le> {
    fn get_local_name<'a>(&'a self) -> &'a str {
        self.element.tag_name.as_slice()
    }

    fn get_namespace_url<'a>(&'a self) -> &'a str {
        self.element.namespace.to_str().unwrap_or("")
    }

    fn get_attr(&self, ns_url: Option<~str>, name: &str) -> Option<&'static str> {
        let namespace = Namespace::from_str(ns_url);
        unsafe { self.element.get_attr_val_for_layout(namespace, name) }
    }

    fn get_link(&self) -> Option<~str> {
        // FIXME: This is HTML only.
        match self.element.node.type_id {
            // http://www.whatwg.org/specs/web-apps/current-work/multipage/selectors.html#
            // selector-link
            ElementNodeTypeId(HTMLAnchorElementTypeId) |
            ElementNodeTypeId(HTMLAreaElementTypeId) |
            ElementNodeTypeId(HTMLLinkElementTypeId) => {
                self.get_attr(None, "href").map(|val| val.to_owned())
            }
            _ => None,
        }
    }
}

