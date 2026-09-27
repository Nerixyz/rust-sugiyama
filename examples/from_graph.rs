use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use rust_sugiyama::{configure::Config, from_graph};
use std::{collections::HashMap, fmt::Write};
use svg::{
    node::{
        element::{Path, Rectangle, Text},
        Comment,
    },
    Document,
};

use crate::path_builder::SvgPath;

fn main() {
    let mut g: StableDiGraph<String, usize> = StableDiGraph::new();

    let rick = g.add_node("Rick".to_string());
    let morty = g.add_node("Morty".to_string());
    let beth = g.add_node("Beth".to_string());
    let jerry = g.add_node("Jerry".to_string());
    let summer = g.add_node("Summer".to_string());
    let foo = g.add_node("Foo".to_string());
    let bar = g.add_node("Bar".to_string());
    let baz = g.add_node("Baz".to_string());

    g.add_edge(rick, beth, 1);
    g.add_edge(rick, jerry, 1);
    g.add_edge(beth, summer, 1);
    g.add_edge(jerry, summer, 1);
    g.add_edge(beth, morty, 1);
    g.add_edge(jerry, morty, 1);
    g.add_edge(summer, foo, 1);
    g.add_edge(foo, bar, 1);
    g.add_edge(bar, baz, 1);
    g.add_edge(summer, baz, 1);

    let vertex_size = |_, text: &String| (text.len() as f64 * 15.0, 25.0);

    let layouts = from_graph(
        &g,
        &vertex_size,
        &Config {
            vertex_spacing: 10.0,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|(layout, edges, width, height)| {
        println!(
            "{}",
            create_svg(&g, &layout, &edges, 15.0, 25.0, (10.0, 10.0))
        );

        let mut new_layout = HashMap::new();
        for (id, coords) in layout {
            new_layout.insert(g[NodeIndex::from(id)].clone(), coords);
        }
        (new_layout, width, height)
    })
    .collect::<Vec<_>>();

    for (layout, width, height) in layouts {
        println!("Coordinates: {:?}", layout);
        println!("width: {width}, height: {height}");
    }
}

/// Generates an SVG string of the resulting layout. This is an example of
/// rendering the layouts.
fn create_svg(
    graph: &StableDiGraph<String, usize>,
    layout: &[(NodeIndex, (f64, f64))],
    edges: &[((NodeIndex, NodeIndex), Vec<(f64, f64)>)],
    character_width: f64,
    node_height: f64,
    padding: (f64, f64),
) -> String {
    // Figure out the extents of the SVG.
    let node_size = |idx: NodeIndex| {
        (
            graph.node_weight(idx).unwrap().len() as f64 * character_width,
            node_height,
        )
    };
    let f64_cmp = |a: &f64, b: &f64| a.partial_cmp(b).unwrap();
    let min_x = layout
        .iter()
        .map(|(idx, (x, _))| *x - node_size(*idx).0 * 0.5)
        .min_by(f64_cmp)
        .unwrap();
    let max_x = layout
        .iter()
        .map(|(idx, (x, _))| *x + node_size(*idx).0 * 0.5)
        .max_by(f64_cmp)
        .unwrap();
    let min_y = layout
        .iter()
        .map(|(idx, (_, y))| *y - node_size(*idx).1 * 0.5)
        .min_by(f64_cmp)
        .unwrap();
    let max_y = layout
        .iter()
        .map(|(idx, (_, y))| *y + node_size(*idx).1 * 0.5)
        .max_by(f64_cmp)
        .unwrap();

    let mut document = Document::new()
        .set("width", max_x - min_x + 2.0 * padding.0)
        .set("height", max_y - min_y + 2.0 * padding.1);

    let origin = (padding.0 - min_x, padding.1 - min_y);

    // Add all the edges.
    dbg!(graph);
    for ((src_ix, dst_ix), points) in edges {
        dbg!((src_ix, dst_ix));
        let mut d = path_builder::SvgPath::new();
        let mut lb = path_builder::MonotoneY::new(&mut d);
        lb.line_start();
        for &(x, y) in points {
            lb.point(x + origin.0, y + origin.1);
        }
        lb.line_end();
        let path = Path::new()
            .set("d", d.into_string())
            .set("style", "stroke:black;fill:none;")
            .add(Comment::new(format!(
                "({}, {})",
                graph.node_weight(*src_ix).unwrap(),
                graph.node_weight(*dst_ix).unwrap()
            )));
        document = document.add(path);
    }

    // Generate a rectangle for each edge in the graph.
    for (idx, pos) in layout {
        let vertex_name = graph.node_weight(*idx).unwrap();

        let size = (vertex_name.len() as f64 * character_width, node_height);

        let rect = Rectangle::new()
            .set("x", pos.0 - size.0 * 0.5 + origin.0)
            .set("y", pos.1 - size.1 * 0.5 + origin.1)
            .set("width", size.0)
            .set("height", size.1)
            .set("style", "fill:white;stroke:black;");

        let text = Text::new(vertex_name)
            .set("x", pos.0 + origin.0)
            .set("y", pos.1 + origin.1)
            .set("dominant-baseline", "middle")
            .set("text-anchor", "middle");
        document = document.add(rect).add(text);
    }

    document.to_string()
}

mod path_builder {
    use std::fmt::Write;

    /// A sink for path commands. This is the `_context` from the JavaScript code.
    pub trait PathContext {
        fn move_to(&mut self, x: f64, y: f64);
        fn line_to(&mut self, x: f64, y: f64);
        fn bezier_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64);
        fn close_path(&mut self);
    }

    /// Convenience impl so `MonotoneX::new(&mut svg)` works.
    impl<T: PathContext + ?Sized> PathContext for &mut T {
        fn move_to(&mut self, x: f64, y: f64) {
            (**self).move_to(x, y);
        }

        fn line_to(&mut self, x: f64, y: f64) {
            (**self).line_to(x, y);
        }

        fn bezier_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
            (**self).bezier_curve_to(x1, y1, x2, y2, x, y);
        }

        fn close_path(&mut self) {
            (**self).close_path();
        }
    }

    #[inline]
    fn sign(x: f64) -> f64 {
        if x < 0.0 {
            -1.0
        } else {
            1.0
        }
    }

    /// JavaScript `Math.min` returns `NaN` if any argument is `NaN`.
    fn js_min3(a: f64, b: f64, c: f64) -> f64 {
        if a.is_nan() || b.is_nan() || c.is_nan() {
            f64::NAN
        } else {
            a.min(b).min(c)
        }
    }

    #[derive(Clone, Debug)]
    pub struct MonotoneX<C> {
        context: C,
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
        t0: f64,
        point: u8,
        line: f64,
    }

    impl<C: PathContext> MonotoneX<C> {
        pub fn new(context: C) -> Self {
            Self {
                context,
                x0: f64::NAN,
                x1: f64::NAN,
                y0: f64::NAN,
                y1: f64::NAN,
                t0: f64::NAN,
                point: 0,
                // `NaN` is used here to model JavaScript `undefined`/`NaN` for `_line`.
                line: f64::NAN,
            }
        }

        pub fn line_start(&mut self) {
            self.x0 = f64::NAN;
            self.x1 = f64::NAN;
            self.y0 = f64::NAN;
            self.y1 = f64::NAN;
            self.t0 = f64::NAN;
            self.point = 0;
        }

        pub fn line_end(&mut self) {
            match self.point {
                2 => self.context.line_to(self.x1, self.y1),
                3 => {
                    let t1 = self.slope2(self.t0);
                    self.draw_point(self.t0, t1);
                }
                _ => {}
            }

            if self.line != 0.0 && (!self.line.is_nan() || self.point == 1) {
                self.context.close_path();
            }

            self.line = 1.0 - self.line;
        }

        pub fn point(&mut self, x: f64, y: f64) {
            let mut t1 = f64::NAN;

            // Ignore coincident points.
            if x == self.x1 && y == self.y1 {
                return;
            }

            match self.point {
                0 => {
                    self.point = 1;
                    if self.line != 0.0 && !self.line.is_nan() {
                        self.context.line_to(x, y);
                    } else {
                        self.context.move_to(x, y);
                    }
                }
                1 => {
                    self.point = 2;
                }
                2 => {
                    self.point = 3;
                    t1 = self.slope3(x, y);
                    let t0 = self.slope2(t1);
                    self.draw_point(t0, t1);
                }
                _ => {
                    t1 = self.slope3(x, y);
                    self.draw_point(self.t0, t1);
                }
            }

            self.x0 = self.x1;
            self.x1 = x;
            self.y0 = self.y1;
            self.y1 = y;
            self.t0 = t1;
        }

        fn slope3(&self, x2: f64, y2: f64) -> f64 {
            let h0 = self.x1 - self.x0;
            let h1 = x2 - self.x1;

            // JavaScript: h0 || h1 < 0 && -0
            let denom0 = if h0 != 0.0 && !h0.is_nan() {
                h0
            } else if h1 < 0.0 {
                -0.0
            } else {
                0.0
            };

            // JavaScript: h1 || h0 < 0 && -0
            let denom1 = if h1 != 0.0 && !h1.is_nan() {
                h1
            } else if h0 < 0.0 {
                -0.0
            } else {
                0.0
            };

            let s0 = (self.y1 - self.y0) / denom0;
            let s1 = (y2 - self.y1) / denom1;
            let p = (s0 * h1 + s1 * h0) / (h0 + h1);

            let v = (sign(s0) + sign(s1)) * js_min3(s0.abs(), s1.abs(), 0.5 * p.abs());

            if v == 0.0 || v.is_nan() {
                0.0
            } else {
                v
            }
        }

        fn slope2(&self, t: f64) -> f64 {
            let h = self.x1 - self.x0;
            if h != 0.0 && !h.is_nan() {
                (3.0 * (self.y1 - self.y0) / h - t) / 2.0
            } else {
                t
            }
        }

        fn draw_point(&mut self, t0: f64, t1: f64) {
            let x0 = self.x0;
            let y0 = self.y0;
            let x1 = self.x1;
            let y1 = self.y1;
            let dx = (x1 - x0) / 3.0;

            self.context
                .bezier_curve_to(x0 + dx, y0 + dx * t0, x1 - dx, y1 - dx * t1, x1, y1);
        }
    }

    /// Swaps x/y coordinates before forwarding to the wrapped context.
    #[derive(Clone, Debug)]
    struct SwapXY<C>(C);

    impl<C: PathContext> PathContext for SwapXY<C> {
        fn move_to(&mut self, x: f64, y: f64) {
            self.0.move_to(y, x);
        }

        fn line_to(&mut self, x: f64, y: f64) {
            self.0.line_to(y, x);
        }

        fn bezier_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
            self.0.bezier_curve_to(y1, x1, y2, x2, y, x);
        }

        fn close_path(&mut self) {
            self.0.close_path();
        }
    }

    #[derive(Clone, Debug)]
    pub struct MonotoneY<C> {
        inner: MonotoneX<SwapXY<C>>,
    }

    impl<C: PathContext> MonotoneY<C> {
        pub fn new(context: C) -> Self {
            Self {
                inner: MonotoneX::new(SwapXY(context)),
            }
        }

        pub fn line_start(&mut self) {
            self.inner.line_start();
        }

        pub fn line_end(&mut self) {
            self.inner.line_end();
        }

        pub fn point(&mut self, x: f64, y: f64) {
            self.inner.point(y, x);
        }
    }

    /// A simple SVG path string builder implementing `PathContext`.
    #[derive(Default, Clone, Debug)]
    pub struct SvgPath {
        d: String,
    }

    impl SvgPath {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn into_string(self) -> String {
            self.d
        }

        fn sep(&mut self) {
            if !self.d.is_empty() {
                self.d.push(' ');
            }
        }
    }

    impl PathContext for SvgPath {
        fn move_to(&mut self, x: f64, y: f64) {
            self.sep();
            write!(self.d, "M {} {}", x, y).unwrap();
        }

        fn line_to(&mut self, x: f64, y: f64) {
            self.sep();
            write!(self.d, "L {} {}", x, y).unwrap();
        }

        fn bezier_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
            self.sep();
            write!(self.d, "C {} {} {} {} {} {}", x1, y1, x2, y2, x, y).unwrap();
        }

        fn close_path(&mut self) {
            self.sep();
            self.d.push('Z');
        }
    }
}
