use im::{Vector, vector};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

pub type SpanSpec = (String, Option<Style>);
pub type LineSpec = Vector<SpanSpec>;

#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphBuilder {
    spans: Vector<SpanSpec>,
    lines: Vector<LineSpec>,
    border: Option<String>,
    start_line: usize,
}

impl ParagraphBuilder {
    pub fn new() -> Self {
        Self {
            spans: vector!(),
            lines: vector!(),
            border: None,
            start_line: 0,
        }
    }

    pub fn start_line(&mut self, start: usize) -> &mut Self {
        self.start_line = start;
        self
    }

    pub fn add_plain(&mut self, s: String) -> &mut Self {
        self.add((s, None))
    }

    pub fn add_styled(&mut self, s: String, style: Style) -> &mut Self {
        self.add((s, Some(style)))
    }

    pub fn add(&mut self, spec: SpanSpec) -> &mut Self {
        self.spans.push_back(spec);
        self
    }

    pub fn new_line(&mut self) -> &mut Self {
        self.lines.push_back(self.spans.clone());
        self.spans.clear();
        self
    }

    pub fn bordered(&mut self) -> &mut Self {
        self.border = Some(String::new());
        self
    }

    pub fn titled(&mut self, title: String) -> &mut Self {
        self.border = Some(title);
        self
    }

    pub fn build(&self) -> Paragraph {
        let line_count = Ord::max(1, self.lines.len());
        let skip = Ord::min(line_count - 1, self.start_line);
        let lines: Vec<Line> = self
            .lines
            .iter()
            .skip(skip)
            .map(|spec| Self::build_line(spec))
            .collect();
        let para = Paragraph::new(lines);
        match &self.border {
            Some(title) if title.is_empty() => para.block(Block::bordered()),
            Some(title) => para.block(Block::bordered().title(title.to_string())),
            None => para,
        }
    }

    fn build_line<'a>(spans: &'a Vector<SpanSpec>) -> Line<'a> {
        let spans: Vec<Span<'a>> = spans
            .iter()
            .map(|(t, s)| match s {
                Some(style) => Span::styled(t, *style),
                None => Span::raw(t),
            })
            .collect();
        Line::from(spans)
    }
}

impl Widget for ParagraphBuilder {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.build().render(area, buf)
    }
}
