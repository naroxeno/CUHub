//! The frameless window's edge and corner strips, which bring back resizing.

use nsh_core::desktop::ResizeEdge;
use nsh_core::prelude::*;

use crate::app_state::Ui;
use crate::components::scaled;

const EDGE_BASE: f32 = 6.0;
const CORNER_BASE: f32 = 10.0;

pub fn frame_edges(ui: &Ui) -> Vec<AnyElement> {
    let z = ui.zoom;
    let edge = scaled(&z, EDGE_BASE);
    let corner = scaled(&z, CORNER_BASE);
    vec![
        view()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(edge.clone())
            .window_resize(ResizeEdge::Top)
            .into_element(),
        view()
            .absolute()
            .bottom(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(edge.clone())
            .window_resize(ResizeEdge::Bottom)
            .into_element(),
        view()
            .absolute()
            .top(px(0.0))
            .bottom(px(0.0))
            .left(px(0.0))
            .w(edge.clone())
            .window_resize(ResizeEdge::Left)
            .into_element(),
        view()
            .absolute()
            .top(px(0.0))
            .bottom(px(0.0))
            .right(px(0.0))
            .w(edge.clone())
            .window_resize(ResizeEdge::Right)
            .into_element(),
        view()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(corner.clone())
            .h(corner.clone())
            .window_resize(ResizeEdge::TopLeft)
            .into_element(),
        view()
            .absolute()
            .top(px(0.0))
            .right(px(0.0))
            .w(corner.clone())
            .h(corner.clone())
            .window_resize(ResizeEdge::TopRight)
            .into_element(),
        view()
            .absolute()
            .bottom(px(0.0))
            .left(px(0.0))
            .w(corner.clone())
            .h(corner.clone())
            .window_resize(ResizeEdge::BottomLeft)
            .into_element(),
        view()
            .absolute()
            .bottom(px(0.0))
            .right(px(0.0))
            .w(corner.clone())
            .h(corner)
            .window_resize(ResizeEdge::BottomRight)
            .into_element(),
    ]
}
