#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

#[derive(Clone, Debug)]
enum Token {
    Command(char),
    Number(f32),
}

struct Lexer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn next(&mut self) -> Option<Token> {
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            if byte.is_ascii_whitespace() || byte == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let byte = *self.bytes.get(self.pos)?;
        if byte.is_ascii_alphabetic() && !matches!(byte, b'e' | b'E') {
            self.pos += 1;
            return Some(Token::Command(byte as char));
        }
        if !byte.is_ascii_digit() && !matches!(byte, b'.' | b'+' | b'-') {
            self.pos += 1;
            return None;
        }

        let start = self.pos;
        if matches!(byte, b'+' | b'-') {
            self.pos += 1;
        }
        let mut has_digits = false;
        let mut has_dot = false;
        let mut has_exponent = false;
        while self.pos < self.bytes.len() {
            let ch = self.bytes[self.pos];
            if ch.is_ascii_digit() {
                has_digits = true;
                self.pos += 1;
                continue;
            }
            if ch == b'.' && !has_dot && !has_exponent {
                has_dot = true;
                self.pos += 1;
                continue;
            }
            if matches!(ch, b'e' | b'E') && has_digits && !has_exponent {
                let mut next = self.pos + 1;
                if matches!(self.bytes.get(next), Some(b'+' | b'-')) {
                    next += 1;
                }
                if matches!(self.bytes.get(next), Some(byte) if byte.is_ascii_digit()) {
                    self.pos = next;
                    has_exponent = true;
                    continue;
                }
            }
            break;
        }
        if !has_digits {
            self.pos += 1;
            return None;
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()?
            .parse::<f32>()
            .ok()
            .map(Token::Number)
    }
}

struct Reader<'a> {
    lexer: Lexer<'a>,
    pending: Option<Token>,
}

impl<'a> Reader<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lexer: Lexer::new(input),
            pending: None,
        }
    }

    fn next(&mut self) -> Option<Token> {
        if let Some(token) = self.pending.take() {
            return Some(token);
        }
        self.lexer.next()
    }

    fn peek(&mut self) -> Option<Token> {
        let token = self.lexer.next();
        self.pending = token.clone();
        token
    }

    fn number(&mut self) -> Option<f32> {
        match self.next() {
            Some(Token::Number(value)) => Some(value),
            _ => None,
        }
    }
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn numbers(value: &str) -> Vec<f32> {
    let mut lexer = Lexer::new(value);
    let mut out = Vec::new();
    while let Some(token) = lexer.next() {
        if let Token::Number(number) = token {
            out.push(number);
        }
    }
    out
}

pub fn parse(svg: &str) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if let Some(d) = attribute(tag, "d") {
            out.extend(parse_path(&d));
        } else if tag.starts_with("circle") {
            let cx = attribute(tag, "cx")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let cy = attribute(tag, "cy")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let r = attribute(tag, "r")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            out.push(circle(cx, cy, r));
        } else if tag.starts_with("rect") {
            let x = attribute(tag, "x")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let y = attribute(tag, "y")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let width = attribute(tag, "width")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let height = attribute(tag, "height")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            out.push(vec![
                Point::new(x, y),
                Point::new(x + width, y),
                Point::new(x + width, y + height),
                Point::new(x, y + height),
                Point::new(x, y),
            ]);
        } else if tag.starts_with("line") {
            let x1 = attribute(tag, "x1")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let y1 = attribute(tag, "y1")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let x2 = attribute(tag, "x2")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let y2 = attribute(tag, "y2")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            out.push(vec![Point::new(x1, y1), Point::new(x2, y2)]);
        } else if tag.starts_with("polyline") || tag.starts_with("polygon") {
            let values = numbers(&attribute(tag, "points").unwrap_or_default());
            let mut points = Vec::new();
            for pair in values.as_chunks::<2>().0 {
                points.push(Point::new(pair[0], pair[1]));
            }
            if tag.starts_with("polygon") {
                if let Some(first) = points.first().copied() {
                    points.push(first);
                }
            }
            if !points.is_empty() {
                out.push(points);
            }
        }
    }
    out
}

fn circle(cx: f32, cy: f32, r: f32) -> Vec<Point> {
    let steps = 32;
    (0..=steps)
        .map(|index| {
            let angle = index as f32 / steps as f32 * std::f32::consts::TAU;
            Point::new(cx + r * angle.cos(), cy + r * angle.sin())
        })
        .collect()
}

fn cubic(a: Point, b: Point, c: Point, d: Point) -> Vec<Point> {
    let steps = 8;
    (1..=steps)
        .map(|index| {
            let t = index as f32 / steps as f32;
            let u = 1.0 - t;
            Point::new(
                u * u * u * a.x + 3.0 * u * u * t * b.x + 3.0 * u * t * t * c.x + t * t * t * d.x,
                u * u * u * a.y + 3.0 * u * u * t * b.y + 3.0 * u * t * t * c.y + t * t * t * d.y,
            )
        })
        .collect()
}

fn quadratic(a: Point, b: Point, c: Point) -> Vec<Point> {
    let steps = 8;
    (1..=steps)
        .map(|index| {
            let t = index as f32 / steps as f32;
            let u = 1.0 - t;
            Point::new(
                u * u * a.x + 2.0 * u * t * b.x + t * t * c.x,
                u * u * a.y + 2.0 * u * t * b.y + t * t * c.y,
            )
        })
        .collect()
}

fn arc(
    start: Point,
    end: Point,
    rx: f32,
    ry: f32,
    phi: f32,
    large: bool,
    sweep: bool,
) -> Vec<Point> {
    if rx.abs() < 0.001 || ry.abs() < 0.001 || start == end {
        return vec![end];
    }
    let phi = phi.to_radians();
    let (cos_phi, sin_phi) = (phi.cos(), phi.sin());
    let dx = (start.x - end.x) * 0.5;
    let dy = (start.y - end.y) * 0.5;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    let lambda = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
    }
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let numerator = rx * rx * ry * ry - denominator;
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let coefficient = sign * (numerator / denominator.max(0.000_001)).max(0.0).sqrt();
    let cx1 = coefficient * rx * y1 / ry;
    let cy1 = coefficient * -ry * x1 / rx;
    let cx = cos_phi * cx1 - sin_phi * cy1 + (start.x + end.x) * 0.5;
    let cy = sin_phi * cx1 + cos_phi * cy1 + (start.y + end.y) * 0.5;
    let theta1 = ((y1 - cy1) / ry).atan2((x1 - cx1) / rx);
    let mut delta = ((-y1 - cy1) / ry).atan2((-x1 - cx1) / rx) - theta1;
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }
    let steps = (delta.abs() / (std::f32::consts::PI / 12.0))
        .ceil()
        .max(1.0) as usize;
    (1..=steps)
        .map(|index| {
            let theta = theta1 + delta * index as f32 / steps as f32;
            let x = rx * theta.cos();
            let y = ry * theta.sin();
            Point::new(
                cx + cos_phi * x - sin_phi * y,
                cy + sin_phi * x + cos_phi * y,
            )
        })
        .collect()
}

fn parse_path(d: &str) -> Vec<Vec<Point>> {
    macro_rules! num {
        ($reader:expr) => {
            match $reader.number() {
                Some(value) => value,
                None => break,
            }
        };
    }
    let mut reader = Reader::new(d);
    let mut out = Vec::new();
    let mut current = Point::new(0.0, 0.0);
    let mut start = current;
    let mut last_cubic: Option<Point> = None;
    let mut last_quadratic: Option<Point> = None;
    while let Some(token) = reader.next() {
        let Token::Command(command) = token else {
            continue;
        };
        let relative = command.is_ascii_lowercase();
        match command.to_ascii_uppercase() {
            'M' => {
                let mut first = true;
                while let Some(x) = reader.number() {
                    let Some(y) = reader.number() else { break };
                    let point = if relative {
                        current.add(Point::new(x, y))
                    } else {
                        Point::new(x, y)
                    };
                    if first {
                        start = point;
                        out.push(vec![point]);
                        first = false;
                    } else if let Some(path) = out.last_mut() {
                        path.push(point);
                    }
                    current = point;
                    if matches!(reader.peek(), Some(Token::Command(_))) {
                        break;
                    }
                }
                last_cubic = None;
                last_quadratic = None;
            }
            'L' | 'H' | 'V' => {
                while let Some(a) = reader.number() {
                    let point = match command.to_ascii_uppercase() {
                        'H' if relative => Point::new(current.x + a, current.y),
                        'H' => Point::new(a, current.y),
                        'V' if relative => Point::new(current.x, current.y + a),
                        'V' => Point::new(current.x, a),
                        _ if relative => current.add(Point::new(a, num!(reader))),
                        _ => Point::new(a, num!(reader)),
                    };
                    if let Some(path) = out.last_mut() {
                        path.push(point);
                    }
                    current = point;
                    if matches!(reader.peek(), Some(Token::Command(_))) {
                        break;
                    }
                }
                last_cubic = None;
                last_quadratic = None;
            }
            'C' | 'S' => loop {
                let (c1, c2, end) = if command.eq_ignore_ascii_case(&'C') {
                    let c1 = Point::new(num!(reader), num!(reader));
                    let c2 = Point::new(num!(reader), num!(reader));
                    let end = Point::new(num!(reader), num!(reader));
                    if relative {
                        (c1.add(current), c2.add(current), end.add(current))
                    } else {
                        (c1, c2, end)
                    }
                } else {
                    let c2 = Point::new(num!(reader), num!(reader));
                    let end = Point::new(num!(reader), num!(reader));
                    let c1 = last_cubic
                        .filter(|_| matches!(command, 'S' | 's'))
                        .map(|control| {
                            Point::new(2.0 * current.x - control.x, 2.0 * current.y - control.y)
                        })
                        .unwrap_or(current);
                    if relative {
                        (c1, c2.add(current), end.add(current))
                    } else {
                        (c1, c2, end)
                    }
                };
                if let Some(path) = out.last_mut() {
                    path.extend(cubic(current, c1, c2, end));
                }
                current = end;
                last_cubic = Some(c2);
                last_quadratic = None;
                if matches!(reader.peek(), Some(Token::Command(_))) {
                    break;
                }
            },
            'Q' | 'T' => loop {
                let (control, end) = if command.eq_ignore_ascii_case(&'Q') {
                    let control = Point::new(num!(reader), num!(reader));
                    let end = Point::new(num!(reader), num!(reader));
                    if relative {
                        (control.add(current), end.add(current))
                    } else {
                        (control, end)
                    }
                } else {
                    let end = Point::new(num!(reader), num!(reader));
                    let control = last_quadratic
                        .filter(|_| matches!(command, 'T' | 't'))
                        .map(|point| {
                            Point::new(2.0 * current.x - point.x, 2.0 * current.y - point.y)
                        })
                        .unwrap_or(current);
                    if relative {
                        (control, end.add(current))
                    } else {
                        (control, end)
                    }
                };
                if let Some(path) = out.last_mut() {
                    path.extend(quadratic(current, control, end));
                }
                current = end;
                last_quadratic = Some(control);
                last_cubic = None;
                if matches!(reader.peek(), Some(Token::Command(_))) {
                    break;
                }
            },
            'A' => {
                while let Some(rx) = reader.number() {
                    let ry = num!(reader);
                    let phi = num!(reader);
                    let large = num!(reader) != 0.0;
                    let sweep = num!(reader) != 0.0;
                    let end = Point::new(num!(reader), num!(reader));
                    let end = if relative { current.add(end) } else { end };
                    if let Some(path) = out.last_mut() {
                        path.extend(arc(current, end, rx, ry, phi, large, sweep));
                    }
                    current = end;
                    last_cubic = None;
                    last_quadratic = None;
                    if matches!(reader.peek(), Some(Token::Command(_))) {
                        break;
                    }
                }
            }
            'Z' => {
                if let Some(path) = out.last_mut() {
                    path.push(start);
                }
                current = start;
            }
            _ => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lucide_search_svg() {
        let svg = rb_ui::lucide::LucideIcon::Search.svg_str();
        let paths = parse(svg);
        assert!(paths.len() >= 2);
        assert!(paths.iter().flatten().any(|point| point.x != point.y));
    }

    #[test]
    fn parses_compact_negative_numbers() {
        let paths = parse_path("m21 21-4.34-4.34");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].len(), 2);
        assert_eq!(paths[0][0], Point::new(21.0, 21.0));
        assert!((paths[0][1].x - 16.66).abs() < 0.001);
        assert!((paths[0][1].y - 16.66).abs() < 0.001);
    }

    #[test]
    fn parses_arc_commands() {
        let paths = parse_path("M7 11V7a5 5 0 0 1 10 0v4");
        assert!(paths.len() == 1);
        assert!(paths[0].len() > 10);
    }
}
