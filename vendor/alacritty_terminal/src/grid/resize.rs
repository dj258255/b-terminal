//! Grid resize and reflow.

use std::cmp::{Ordering, max, min};
use std::mem;

use crate::index::{Boundary, Column, Line, Point};
use crate::term::cell::{Flags, ResetDiscriminant};

use crate::grid::row::Row;
use crate::grid::{Dimensions, Grid, GridCell};

/// 저장 커서가 있는 한 줄(이어진 줄들)의 자리
#[derive(Clone, Copy)]
struct SavedSpot {
    /// 그 아래에 있는 한 줄의 수
    below: usize,
    /// 그 위에 있는 한 줄의 수(스크롤백 포함)
    above: usize,
    /// 한 줄 안의 칸 위치
    offset: usize,
}

impl<T: GridCell + Default + PartialEq> Grid<T> {
    /// Resize the grid's width and/or height.
    pub fn resize<D>(&mut self, reflow: bool, lines: usize, columns: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        self.resize_with_lost_wraps(reflow, lines, columns, 0);
    }

    /// `resize`에, 크기를 바꾸기 전에 비운 프롬프트 줄 중 "윗줄이 이어지던 줄"의 수(`lost_wraps`)를 더한 것.
    /// b-terminal(#583): Ghostty는 줄을 줄일 때 커서 아래 빈 줄을 "커서 위 이어진 줄(`wrap_continuation`)이 늘어난 만큼"만 씀
    /// (PageList.zig 1651~1675줄 `preserved_cursor`). 비운 줄은 이어진 줄 표시를 잃으므로 그만큼은 늘어난 줄로 치지 않는다
    pub fn resize_with_lost_wraps<D>(&mut self, reflow: bool, lines: usize, columns: usize, lost_wraps: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // Use empty template cell for resetting cells due to resize.
        let template = mem::take(&mut self.cursor.template);

        match self.lines.cmp(&lines) {
            Ordering::Less => self.grow_lines(lines),
            Ordering::Greater => self.shrink_lines(lines),
            Ordering::Equal => (),
        }

        // b-terminal(#537): 저장 커서도 다시 흘릴 때 따라간다(Ghostty `Screen.resize`의 `saved_cursor_pin`).
        // 줄 수를 바꾼 뒤의 구조에서 자리를 적어 둔다
        let growing = self.columns < columns;
        let saved_spot = if self.columns != columns && reflow { self.saved_cursor_spot() } else { None };

        let mut trimmed = 0;
        match self.columns.cmp(&columns) {
            Ordering::Less => self.grow_columns(reflow, columns),
            Ordering::Greater => trimmed = self.shrink_columns(reflow, columns, saved_spot.map(|s| s.below), lost_wraps),
            Ordering::Equal => (),
        }

        if let Some(spot) = saved_spot {
            self.restore_saved_cursor_spot(spot, trimmed, growing);
        }

        // b-terminal(#537): 줄바꿈 대기인 커서가 크기를 바꾼 뒤 줄 끝이 아니면 대기를 풀고 한 칸 오른쪽으로 간다.
        // 저장 커서도 같다(Ghostty `Screen.zig` 2240·2282줄)
        let last = Column(columns - 1);
        if self.cursor.input_needs_wrap && self.cursor.point.column != last {
            self.cursor.input_needs_wrap = false;
            self.cursor.point.column = min(self.cursor.point.column + 1, last);
        }
        if self.saved_cursor.input_needs_wrap && self.saved_cursor.point.column != last {
            self.saved_cursor.input_needs_wrap = false;
            self.saved_cursor.point.column = min(self.saved_cursor.point.column + 1, last);
        }

        // Restore template cell.
        self.cursor.template = template;
    }

    /// Add lines to the visible area.
    ///
    /// Alacritty keeps the cursor at the bottom of the terminal as long as there
    /// is scrollback available. Once scrollback is exhausted, new lines are
    /// simply added to the bottom of the screen.
    fn grow_lines<D>(&mut self, target: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        let lines_added = target - self.lines;

        // Need to resize before updating buffer.
        self.raw.grow_visible_lines(target);
        self.lines = target;

        // b-terminal(#584): 커서가 맨 아래 줄이 아니면 스크롤백을 끌어내리지 않는다(Ghostty PageList.zig 3084~3100줄:
        // `pull = cursor.y >= self.rows - 1`, "we don't want to pull down scrollback. This is purely a UX feature").
        // 올려 본 화면도 그 자리에 남고(`display_offset`은 `scroll_up`이 맞춘다), 커서 줄도 그대로다
        let pull = self.cursor.point.line.0 as usize + 1 >= self.lines - lines_added;
        let history_size = self.history_size();
        let from_history = if pull { min(history_size, lines_added) } else { 0 };

        // Move existing lines up for every line that couldn't be pulled from history.
        if from_history != lines_added {
            let delta = lines_added - from_history;
            self.scroll_up(&(Line(0)..Line(target as i32)), delta);
        }

        // Move cursor down for every line pulled from history.
        self.saved_cursor.point.line += from_history;
        self.cursor.point.line += from_history;

        if pull {
            self.display_offset = self.display_offset.saturating_sub(lines_added);
        }
        self.decrease_scroll_limit(lines_added);
    }

    /// Remove lines from the visible area.
    ///
    /// The behavior in Terminal.app and iTerm.app is to keep the cursor at the
    /// bottom of the screen. This is achieved by pushing history "out the top"
    /// of the terminal window.
    ///
    /// Alacritty takes the same approach.
    fn shrink_lines<D>(&mut self, target: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // Scroll up to keep content inside the window.
        let required_scrolling = (self.cursor.point.line.0 as usize + 1).saturating_sub(target);
        if required_scrolling > 0 {
            self.scroll_up(&(Line(0)..Line(self.lines as i32)), required_scrolling);

            // Clamp cursors to the new viewport size.
            self.cursor.point.line = min(self.cursor.point.line, Line(target as i32 - 1));
        }

        // Clamp saved cursor, since only primary cursor is scrolled into viewport.
        self.saved_cursor.point.line = min(self.saved_cursor.point.line, Line(target as i32 - 1));

        self.raw.rotate((self.lines - target) as isize);
        self.raw.shrink_visible_lines(target);
        self.lines = target;
    }

    /// Grow number of columns in each row, reflowing if necessary.
    fn grow_columns(&mut self, reflow: bool, columns: usize) {
        // Check if a row needs to be wrapped.
        let should_reflow = |row: &Row<T>| -> bool {
            let len = Column(row.len());
            reflow && len.0 > 0 && len < columns && row[len - 1].flags().contains(Flags::WRAPLINE)
        };

        self.columns = columns;

        let mut reversed: Vec<Row<T>> = Vec::with_capacity(self.raw.len());
        let mut cursor_line_delta = 0;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.input_needs_wrap && reflow {
            self.cursor.input_needs_wrap = false;
            self.cursor.point.column += 1;
        }

        let mut rows = self.raw.take_all();

        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Check if reflowing should be performed.
            let last_row = match reversed.last_mut() {
                Some(last_row) if should_reflow(last_row) => last_row,
                _ => {
                    reversed.push(row);
                    continue;
                },
            };

            // Remove wrap flag before appending additional cells.
            if let Some(cell) = last_row.last_mut() {
                cell.flags_mut().remove(Flags::WRAPLINE);
            }

            // Remove leading spacers when reflowing wide char to the previous line.
            let mut last_len = last_row.len();
            if last_len >= 1
                && last_row[Column(last_len - 1)].flags().contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                last_row.shrink(last_len - 1);
                last_len -= 1;
            }

            // Don't try to pull more cells from the next line than available.
            let mut num_wrapped = columns - last_len;
            let len = min(row.len(), num_wrapped);

            // Insert leading spacer when there's not enough room for reflowing wide char.
            let mut cells = if row[Column(len - 1)].flags().contains(Flags::WIDE_CHAR) {
                num_wrapped -= 1;

                let mut cells = row.front_split_off(len - 1);

                let mut spacer = T::default();
                spacer.flags_mut().insert(Flags::LEADING_WIDE_CHAR_SPACER);
                cells.push(spacer);

                cells
            } else {
                row.front_split_off(len)
            };

            // Add removed cells to previous row and reflow content.
            last_row.append(&mut cells);

            let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;

            if i == cursor_buffer_line && reflow {
                // Resize cursor's line and reflow the cursor if necessary.
                let mut target = self.cursor.point.sub(self, Boundary::Cursor, num_wrapped);

                // Clamp to the last column, if no content was reflown with the cursor.
                if target.column.0 == 0 && row.is_clear() {
                    self.cursor.input_needs_wrap = true;
                    target = target.sub(self, Boundary::Cursor, 1);
                }
                self.cursor.point.column = target.column;

                // Get required cursor line changes. Since `num_wrapped` is smaller than `columns`
                // this will always be either `0` or `1`.
                let line_delta = self.cursor.point.line - target.line;

                if line_delta != 0 && row.is_clear() {
                    continue;
                }

                cursor_line_delta += line_delta.0 as usize;
            } else if row.is_clear() {
                if i < self.display_offset {
                    // Since we removed a line, rotate down the viewport.
                    self.display_offset = self.display_offset.saturating_sub(1);
                }

                // Rotate cursor down if content below them was pulled from history.
                if i < cursor_buffer_line {
                    self.cursor.point.line += 1;
                }

                // Don't push line into the new buffer.
                continue;
            }

            if let Some(cell) = last_row.last_mut() {
                // Set wrap flag if next line still has cells.
                cell.flags_mut().insert(Flags::WRAPLINE);
            }

            reversed.push(row);
        }

        // Make sure we have at least the viewport filled.
        if reversed.len() < self.lines {
            let delta = (self.lines - reversed.len()) as i32;
            self.cursor.point.line = max(self.cursor.point.line - delta, Line(0));
            reversed.resize_with(self.lines, || Row::new(columns));
        }

        // Pull content down to put cursor in correct position, or move cursor up if there's no
        // more lines to delete below the cursor.
        if cursor_line_delta != 0 {
            let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
            let available = min(cursor_buffer_line, reversed.len() - self.lines);
            let overflow = cursor_line_delta.saturating_sub(available);
            reversed.truncate(reversed.len() + overflow - cursor_line_delta);
            self.cursor.point.line = max(self.cursor.point.line - overflow, Line(0));
        }

        // Reverse iterator and fill all rows that are still too short.
        let mut new_raw = Vec::with_capacity(reversed.len());
        for mut row in reversed.drain(..).rev() {
            if row.len() < columns {
                row.grow(columns);
            }
            new_raw.push(row);
        }

        self.raw.replace_inner(new_raw);

        // Clamp display offset in case lines above it got merged.
        self.display_offset = min(self.display_offset, self.history_size());
    }

    /// Shrink number of columns in each row, reflowing if necessary.
    ///
    /// b-terminal(#537): 줄이 늘어난 만큼, 커서(와 저장 커서) 아래의 빈 줄을 먼저 없앤다(Ghostty 시험
    /// `Screen: resize less cols with reflow but row space`, Screen.zig 8652줄: 아래에 빈 줄이 있으면 위 줄을 스크롤백으로 밀지 않는다).
    /// 없앤 줄 수를 돌려준다. `protect_below`는 저장 커서의 줄 아래에 있는 한 줄(논리 줄)의 수로, 그보다 많이는 없애지 않는다
    fn shrink_columns(
        &mut self,
        reflow: bool,
        columns: usize,
        protect_below: Option<usize>,
        lost_wraps: usize,
    ) -> usize {
        let old_len = self.raw.len();
        self.columns = columns;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.input_needs_wrap && reflow {
            self.cursor.input_needs_wrap = false;
            self.cursor.point.column += 1;
        }

        let mut new_raw = Vec::with_capacity(self.raw.len());
        let mut buffered: Option<Vec<T>> = None;

        let mut rows = self.raw.take_all();

        // b-terminal(#808): 1열에는 넓은 글자가 들어가지 않는다. 그대로 두면 아래 반복이 넓은 글자를 다음 줄 맨 앞으로 밀고 또 밀어
        // 끝나지 않는다(CPU 100%, 메모리가 계속 늘었다). Ghostty는 이때 넓은 글자를 지운다(시험 `resize less cols to eliminate wide
        // char`, Screen.zig 8974줄·9050줄). 넓은 글자 머리와 자리표시 칸을 빈 칸으로 바꾸고 줄 이음 표시는 남긴다
        if columns == 1 {
            let wide = Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER;
            for row in rows.iter_mut() {
                for c in 0..row.len() {
                    let cell = &mut row[Column(c)];
                    if cell.flags().intersects(wide) {
                        let wrap = *cell.flags() & Flags::WRAPLINE;
                        *cell = T::default();
                        cell.flags_mut().insert(wrap);
                    }
                }
            }
        }

        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Append lines left over from the previous row.
            if let Some(buffered) = buffered.take() {
                // Add a column for every cell added before the cursor, if it goes beyond the new
                // width it is then later reflown.
                let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
                if i == cursor_buffer_line {
                    self.cursor.point.column += buffered.len();
                }

                row.append_front(buffered);
            }

            loop {
                // Remove all cells which require reflowing.
                let mut wrapped = match row.shrink(columns) {
                    Some(wrapped) if reflow => wrapped,
                    _ => {
                        let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
                        if reflow && i == cursor_buffer_line && self.cursor.point.column > columns {
                            // If there are empty cells before the cursor, we assume it is explicit
                            // whitespace and need to wrap it like normal content.
                            Vec::new()
                        } else {
                            // b-terminal(#584): 줄을 다시 흘리지 않고 잘랐을 때(자동 줄바꿈 끔·대체 화면) 마지막 칸에 꼬리를 잃은
                            // 넓은 글자 머리가 남으면 빈 칸으로 바꾼다(Ghostty 시험 `resize less cols without reflow cutting wide char tail`,
                            // Terminal.zig 18659줄)
                            if row.len() >= columns
                                && row[Column(columns - 1)].flags().contains(Flags::WIDE_CHAR)
                            {
                                row[Column(columns - 1)] = T::default();
                            }
                            // Since it fits, just push the existing line without any reflow.
                            new_raw.push(row);
                            break;
                        }
                    },
                };

                // Insert spacer if a wide char would be wrapped into the last column.
                if row.len() >= columns
                    && row[Column(columns - 1)].flags().contains(Flags::WIDE_CHAR)
                {
                    let mut spacer = T::default();
                    spacer.flags_mut().insert(Flags::LEADING_WIDE_CHAR_SPACER);

                    let wide_char = mem::replace(&mut row[Column(columns - 1)], spacer);
                    wrapped.insert(0, wide_char);
                }

                // Remove wide char spacer before shrinking.
                let len = wrapped.len();
                if len > 0 && wrapped[len - 1].flags().contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    if len == 1 {
                        row[Column(columns - 1)].flags_mut().insert(Flags::WRAPLINE);
                        new_raw.push(row);
                        break;
                    } else {
                        // Remove the leading spacer from the end of the wrapped row.
                        wrapped[len - 2].flags_mut().insert(Flags::WRAPLINE);
                        wrapped.truncate(len - 1);
                    }
                }

                new_raw.push(row);

                // Set line as wrapped if cells got removed.
                if let Some(cell) = new_raw.last_mut().and_then(|r| r.last_mut()) {
                    cell.flags_mut().insert(Flags::WRAPLINE);
                }

                if wrapped
                    .last()
                    .map(|c| c.flags().contains(Flags::WRAPLINE) && i >= 1)
                    .unwrap_or(false)
                    && wrapped.len() < columns
                {
                    // Make sure previous wrap flag doesn't linger around.
                    if let Some(cell) = wrapped.last_mut() {
                        cell.flags_mut().remove(Flags::WRAPLINE);
                    }

                    // Add removed cells to start of next row.
                    buffered = Some(wrapped);
                    break;
                } else {
                    // Reflow cursor if a line below it is deleted.
                    let cursor_buffer_line = self.lines - self.cursor.point.line.0 as usize - 1;
                    if (i == cursor_buffer_line && self.cursor.point.column < columns)
                        || i < cursor_buffer_line
                    {
                        self.cursor.point.line = max(self.cursor.point.line - 1, Line(0));
                    }

                    // Reflow the cursor if it is on this line beyond the width.
                    if i == cursor_buffer_line && self.cursor.point.column >= columns {
                        // Since only a single new line is created, we subtract only `columns`
                        // from the cursor instead of reflowing it completely.
                        self.cursor.point.column -= columns;
                    }

                    // Make sure new row is at least as long as new width.
                    let occ = wrapped.len();
                    if occ < columns {
                        wrapped.resize_with(columns, T::default);
                    }
                    row = Row::from_vec(wrapped, occ);

                    if i < self.display_offset {
                        // Since we added a new line, rotate up the viewport.
                        self.display_offset += 1;
                    }
                }
            }
        }

        // Reverse iterator and use it as the new grid storage.
        let mut reversed: Vec<Row<T>> = new_raw.drain(..).rev().collect();

        // 줄이 늘어난 만큼 커서 아래의 빈 줄을 없앤다(저장소는 0번이 맨 아래 줄)
        let mut trimmed = 0;
        if reflow && reversed.len() > old_len {
            let below_cursor = (self.lines as i32 - 1 - self.cursor.point.line.0).max(0) as usize;
            let limit = min(
                min((reversed.len() - old_len).saturating_sub(lost_wraps), below_cursor),
                protect_below.unwrap_or(usize::MAX),
            );
            let wrapped = |row: &Row<T>| {
                row.len() > 0 && row[Column(row.len() - 1)].flags().contains(Flags::WRAPLINE)
            };
            while trimmed < limit
                && reversed[trimmed].is_clear()
                && !wrapped(&reversed[trimmed])
                && reversed.get(trimmed + 1).is_none_or(|above| !wrapped(above))
            {
                trimmed += 1;
            }
            if trimmed > 0 {
                reversed.drain(..trimmed);
                self.cursor.point.line += trimmed;
            }
        }

        reversed.truncate(self.max_scroll_limit + self.lines);
        self.raw.replace_inner(reversed);

        // Clamp display offset in case some lines went off.
        self.display_offset = min(self.display_offset, self.history_size());

        // Reflow the primary cursor, or clamp it if reflow is disabled.
        if !reflow {
            self.cursor.point.column = min(self.cursor.point.column, Column(columns - 1));
        } else if self.cursor.point.column == columns
            && !self[self.cursor.point.line][Column(columns - 1)].flags().contains(Flags::WRAPLINE)
        {
            self.cursor.input_needs_wrap = true;
            self.cursor.point.column -= 1;
        } else {
            self.cursor.point = self.cursor.point.grid_clamp(self, Boundary::Cursor);
        }

        // Clamp the saved cursor to the grid.
        self.saved_cursor.point.column = min(self.saved_cursor.point.column, Column(columns - 1));

        trimmed
    }

    /// 줄 `line`의 마지막 칸이 이어짐 표시(WRAPLINE)를 가졌는가
    fn row_wrapped(&self, line: i32) -> bool {
        let row = &self[Line(line)];
        row.len() > 0 && row[Column(row.len() - 1)].flags().contains(Flags::WRAPLINE)
    }

    /// 줄의 글자 칸 수(끝의 넓은 글자 자리 채움 칸은 뺀다)
    fn row_content_len(&self, line: i32) -> usize {
        let row = &self[Line(line)];
        let n = row.len();
        if n > 0 && row[Column(n - 1)].flags().contains(Flags::LEADING_WIDE_CHAR_SPACER) {
            n - 1
        } else {
            n
        }
    }

    /// b-terminal(#537): 저장 커서가 있는 한 줄(이어진 줄들)을 (그 아래에 있는 한 줄의 수, 한 줄 안의 칸 위치)로 적는다.
    /// 줄을 줄일 때는 아래에서부터 센다(스크롤백이 잘려도 흔들리지 않게). 넓힐 때는 위에서부터 센다
    /// (합쳐진 줄 때문에 맨 아래에 채워 넣는 빈 줄이 아래에서부터 센 값을 어긋나게 한다)
    fn saved_cursor_spot(&self) -> Option<SavedSpot> {
        let point = self.saved_cursor.point;
        let top = -(self.history_size() as i32);
        let bottom = self.lines as i32 - 1;
        if point.line.0 < top || point.line.0 > bottom {
            return None;
        }

        let mut start = point.line.0;
        while start > top && self.row_wrapped(start - 1) {
            start -= 1;
        }
        let mut offset = point.column.0;
        for l in start..point.line.0 {
            offset += self.row_content_len(l);
        }
        let below = ((start + 1)..=bottom).filter(|&l| !self.row_wrapped(l - 1)).count();
        let above = (top..start).filter(|&l| l == top || !self.row_wrapped(l - 1)).count();
        Some(SavedSpot { below, above, offset })
    }

    /// `saved_cursor_spot`로 적어 둔 자리를 다시 흘린 격자에서 찾아 저장 커서에 쓴다.
    /// 활성 영역 밖이면 (0, 0)에 대기 없음(Ghostty `Screen.zig` 2284줄)
    fn restore_saved_cursor_spot(&mut self, spot: SavedSpot, trimmed: usize, growing: bool) {
        let mut offset = spot.offset;
        let below = spot.below.saturating_sub(trimmed);
        let top = -(self.history_size() as i32);
        let bottom = self.lines as i32 - 1;

        let starts: Vec<i32> =
            (top..=bottom).filter(|&l| l == top || !self.row_wrapped(l - 1)).collect();
        let found = if growing {
            starts.get(spot.above).copied()
        } else {
            starts.len().checked_sub(below + 1).map(|i| starts[i])
        };

        let Some(mut line) = found else {
            self.saved_cursor.point = Point::new(Line(0), Column(0));
            self.saved_cursor.input_needs_wrap = false;
            return;
        };

        let last_col = self.columns - 1;
        let column = loop {
            let len = self.row_content_len(line);
            if offset < len || line == bottom || !self.row_wrapped(line) {
                break min(offset, last_col);
            }
            offset -= len;
            line += 1;
        };

        if line < 0 {
            self.saved_cursor.point = Point::new(Line(0), Column(0));
            self.saved_cursor.input_needs_wrap = false;
        } else {
            self.saved_cursor.point = Point::new(Line(line), Column(column));
        }
    }
}
