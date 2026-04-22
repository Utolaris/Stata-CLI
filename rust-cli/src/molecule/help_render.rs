#![allow(dead_code)]

use crate::atom::smcl_html::render_smcl_to_html;

pub(crate) fn render_help_html(smcl: &str) -> String {
    render_smcl_to_html(smcl)
}
