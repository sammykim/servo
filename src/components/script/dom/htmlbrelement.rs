/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use dom::bindings::utils::{DOMString, null_string, ErrorResult};
use dom::htmlelement::HTMLElement;

pub struct HTMLBRElement {
    parent: HTMLElement,
}

impl HTMLBRElement {
    pub fn Clear(&self) -> DOMString {
        null_string
    }

    pub fn SetClear(&mut self, _text: &DOMString, _rv: &mut ErrorResult) {
    }
}
