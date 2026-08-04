use crate::{
    charset::{encode_fixed, supported_text},
    clock::{EMPTY_PROGRESS, format_progress},
    model::{MediaState, TextFrame},
};
use std::time::{Duration, Instant};

pub struct TextComposer {
    width: usize,
    scroll_interval: Duration,
    source_text: String,
    scroll_buffer: Vec<char>,
    offset: usize,
    last_scroll: Instant,
}

impl TextComposer {
    pub fn new(width: usize, scroll_interval_ms: u64) -> Self {
        Self {
            width,
            scroll_interval: Duration::from_millis(scroll_interval_ms.max(100)),
            source_text: String::new(),
            scroll_buffer: Vec::new(),
            offset: 0,
            last_scroll: Instant::now(),
        }
    }

    pub fn reconfigure(&mut self, width: usize, scroll_interval_ms: u64) {
        self.width = width.max(1);
        self.scroll_interval = Duration::from_millis(scroll_interval_ms.max(100));
        self.source_text.clear();
        self.scroll_buffer.clear();
        self.offset = 0;
        self.last_scroll = Instant::now();
    }

    pub fn compose(&mut self, media: &MediaState, separator: &str) -> TextFrame {
        let display_name = if media.info.is_available() {
            supported_text(&media.info.display_name(separator))
        } else {
            String::new()
        };

        self.set_source(display_name);

        if self.scroll_buffer.len() > self.width
            && self.last_scroll.elapsed() >= self.scroll_interval
        {
            self.offset = (self.offset + 1) % self.scroll_buffer.len();

            self.last_scroll = Instant::now();
        }

        let line1 = self.current_line();

        let progress = if media.info.is_available() {
            format_progress(media.current_position(), media.info.duration)
        } else {
            EMPTY_PROGRESS.to_owned()
        };

        TextFrame {
            line1,
            line2: fit_text(&progress, self.width),
        }
    }

    pub fn frame_to_ids(&self, frame: &TextFrame) -> Vec<i32> {
        let mut output = Vec::with_capacity(self.width * 2);

        output.extend(encode_fixed(&frame.line1, self.width));

        output.extend(encode_fixed(&frame.line2, self.width));

        output
    }

    fn set_source(&mut self, text: String) {
        let normalized = text.trim().to_owned();

        if normalized == self.source_text {
            return;
        }

        self.source_text = normalized;
        self.offset = 0;
        self.last_scroll = Instant::now();

        if self.source_text.is_empty() {
            self.scroll_buffer = vec![' '; self.width];

            return;
        }

        self.scroll_buffer = self.source_text.chars().collect();

        if self.scroll_buffer.len() > self.width {
            self.scroll_buffer.extend([' ', ' ', ' ']);
        }
    }

    fn current_line(&self) -> String {
        if self.scroll_buffer.is_empty() {
            return " ".repeat(self.width);
        }

        if self.scroll_buffer.len() <= self.width {
            return fit_chars(&self.scroll_buffer, self.width);
        }

        (0..self.width)
            .map(|index| {
                let position = (self.offset + index) % self.scroll_buffer.len();

                self.scroll_buffer[position]
            })
            .collect()
    }
}

fn fit_text(text: &str, width: usize) -> String {
    let characters = text.chars().collect::<Vec<_>>();

    fit_chars(&characters, width)
}

fn fit_chars(characters: &[char], width: usize) -> String {
    let mut result = characters.iter().take(width).copied().collect::<String>();

    let current = result.chars().count();

    if current < width {
        result.push_str(&" ".repeat(width - current));
    }

    result
}
