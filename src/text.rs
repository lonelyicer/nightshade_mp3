use crate::{
    charset::{encode_fixed, supported_text},
    clock::{EMPTY_PROGRESS, format_progress},
    model::MediaState,
};

use std::time::{Duration, Instant};

const MIN_SCROLL_INTERVAL_MS: u64 = 100;

#[derive(Clone, Debug)]
pub struct DisplayFrame {
    pub characters: Vec<i32>,
    pub title_offset: usize,
}

pub struct TextComposer {
    width: usize,
    scroll_interval: Duration,
    title_gap: usize,

    source_text: String,
    title_ring: Vec<char>,
    title_started_at: Instant,
}

impl TextComposer {
    pub fn new(width: usize, scroll_interval_ms: u64, title_gap: usize) -> Self {
        Self {
            width: width.max(1),

            scroll_interval: Duration::from_millis(scroll_interval_ms.max(MIN_SCROLL_INTERVAL_MS)),

            title_gap: title_gap.max(1),

            source_text: String::new(),

            title_ring: Vec::new(),

            title_started_at: Instant::now(),
        }
    }

    pub fn reconfigure(&mut self, width: usize, scroll_interval_ms: u64, title_gap: usize) {
        self.width = width.max(1);

        self.scroll_interval =
            Duration::from_millis(scroll_interval_ms.max(MIN_SCROLL_INTERVAL_MS));

        self.title_gap = title_gap.max(1);

        self.rebuild_title_ring();

        self.title_started_at = Instant::now();
    }

    pub fn buffer_length(&self) -> usize {
        self.width * 2
    }

    pub fn compose(&mut self, media: &MediaState, separator: &str, now: Instant) -> DisplayFrame {
        let source = if media.info.is_available() {
            supported_text(&media.info.display_name(separator))
        } else {
            String::new()
        };

        self.set_source(source, now);

        let title_offset = self.title_offset(now);

        let title = self.render_title(title_offset);

        let progress = if media.info.is_available() {
            format_progress(media.current_position(), media.info.duration)
        } else {
            EMPTY_PROGRESS.to_owned()
        };

        let progress = fit_text(&progress, self.width);

        let mut unified = String::with_capacity(self.width * 2);

        unified.push_str(&title);
        unified.push_str(&progress);

        DisplayFrame {
            characters: encode_fixed(&unified, self.width * 2),

            title_offset,
        }
    }

    fn set_source(&mut self, source: String, now: Instant) {
        let source = source.trim().to_owned();

        if source == self.source_text {
            return;
        }

        self.source_text = source;

        self.rebuild_title_ring();

        self.title_started_at = now;
    }

    fn rebuild_title_ring(&mut self) {
        self.title_ring = self.source_text.chars().collect();

        if self.title_ring.is_empty() {
            self.title_ring = vec![' '; self.width];

            return;
        }

        if self.title_ring.len() > self.width {
            self.title_ring
                .extend(std::iter::repeat_n(' ', self.title_gap));
        }
    }

    fn title_offset(&self, now: Instant) -> usize {
        if self.title_ring.len() <= self.width {
            return 0;
        }

        let elapsed = now.saturating_duration_since(self.title_started_at);

        let interval_nanos = self.scroll_interval.as_nanos().max(1);

        let elapsed_steps = elapsed.as_nanos() / interval_nanos;

        (elapsed_steps % self.title_ring.len() as u128) as usize
    }

    fn render_title(&self, offset: usize) -> String {
        if self.title_ring.len() <= self.width {
            return fit_chars(&self.title_ring, self.width);
        }

        (0..self.width)
            .map(|index| {
                let position = (offset + index) % self.title_ring.len();

                self.title_ring[position]
            })
            .collect()
    }
}

fn fit_text(text: &str, width: usize) -> String {
    let characters = text.chars().collect::<Vec<_>>();

    fit_chars(&characters, width)
}

fn fit_chars(characters: &[char], width: usize) -> String {
    let mut output = characters.iter().take(width).copied().collect::<String>();

    let length = output.chars().count();

    if length < width {
        output.push_str(&" ".repeat(width - length));
    }

    output
}
