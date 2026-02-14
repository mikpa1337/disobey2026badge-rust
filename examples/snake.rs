//! Snake game for the Disobey 2026 badge.
//!
//! - D-pad to move the snake
//! - Eat food to grow and gain points
//! - Avoid hitting walls and yourself
//! - Press A to start / restart after game over

#![no_std]
#![no_main]

use defmt::info;
#[allow(clippy::wildcard_imports)]
use disobey2026badge::*;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    mono_font::{MonoTextStyle, iso_8859_1::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _;
use palette::Srgb;

extern crate alloc;
use alloc::vec::Vec;

esp_bootloader_esp_idf::esp_app_desc!();

// Display dimensions
const W: i32 = 320;
const H: i32 = 170;

// Grid settings
const GRID_SIZE: i32 = 10;
const GRID_W: i32 = W / GRID_SIZE;
const GRID_H: i32 = H / GRID_SIZE;

// Game parameters
const TICK_MS: u64 = 100;

const SNAKE_COLOR: Rgb565 = Rgb565::GREEN;
const FOOD_COLOR: Rgb565 = Rgb565::RED;

// Simple RNG
struct Rng(u32);
impl Rng {
    const fn new(seed: u32) -> Self { Self(seed) }
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn range(&mut self, max: u32) -> u32 { self.next() % max }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Pos {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    // Prevent moving directly backwards
    fn is_opposite(self, other: Direction) -> bool {
        matches!(
            (self, other),
            (Direction::Up, Direction::Down)
                | (Direction::Down, Direction::Up)
                | (Direction::Left, Direction::Right)
                | (Direction::Right, Direction::Left)
        )
    }
}

struct Game {
    snake: Vec<Pos>,
    direction: Direction,
    next_direction: Direction,
    food: Pos,
    score: u16,
    game_over: bool,
    rng: Rng,
}

impl Game {
    fn new() -> Self {
        let mut game = Self {
            snake: Vec::new(),
            direction: Direction::Right,
            next_direction: Direction::Right,
            food: Pos { x: 0, y: 0 },
            score: 0,
            game_over: false,
            rng: Rng::new(0xDEADBEEF),
        };

        // Initialize snake in the middle
        let start_x = GRID_W / 2;
        let start_y = GRID_H / 2;
        for i in 0..4 {
            game.snake.push(Pos {
                x: start_x - i,
                y: start_y,
            });
        }

        game.spawn_food();
        game
    }

    fn spawn_food(&mut self) {
        loop {
            let x = self.rng.range(GRID_W as u32) as i32;
            let y = self.rng.range(GRID_H as u32) as i32;
            let pos = Pos { x, y };

            // Make sure food doesn't spawn on snake
            if !self.snake.iter().any(|&s| s == pos) {
                self.food = pos;
                break;
            }
        }
    }

    fn tick(&mut self) {
        if self.game_over {
            return;
        }

        // Update direction if valid
        if !self.next_direction.is_opposite(self.direction) {
            self.direction = self.next_direction;
        }

        // Move head
        let head = self.snake[0];
        let new_head = match self.direction {
            Direction::Up => Pos {
                x: head.x,
                y: head.y - 1,
            },
            Direction::Down => Pos {
                x: head.x,
                y: head.y + 1,
            },
            Direction::Left => Pos {
                x: head.x - 1,
                y: head.y,
            },
            Direction::Right => Pos {
                x: head.x + 1,
                y: head.y,
            },
        };

        // Check wall collision
        if new_head.x < 0 || new_head.x >= GRID_W || new_head.y < 0 || new_head.y >= GRID_H {
            self.game_over = true;
            return;
        }

        // Check self collision
        if self.snake.iter().any(|&s| s == new_head) {
            self.game_over = true;
            return;
        }

        // Add new head
        self.snake.insert(0, new_head);

        // Check food collision
        if new_head == self.food {
            self.score += 1;
            self.spawn_food();
        } else {
            // Remove tail if no food eaten
            self.snake.pop();
        }
    }
}

const BLACK: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(Rgb565::BLACK);

fn draw_initial(display: &mut Display, game: &Game) {
    // Clear screen
    Rectangle::new(Point::zero(), Size::new(W as u32, H as u32))
        .into_styled(BLACK)
        .draw(display)
        .unwrap();

    draw_snake(display, game);
    draw_food(display, game);
    draw_hud(display, game.score);
}

fn draw_snake(display: &mut Display, game: &Game) {
    let style = PrimitiveStyle::with_fill(SNAKE_COLOR);
    for segment in &game.snake {
        let x = segment.x * GRID_SIZE;
        let y = segment.y * GRID_SIZE;
        Rectangle::new(
            Point::new(x, y),
            Size::new(GRID_SIZE as u32 - 1, GRID_SIZE as u32 - 1),
        )
        .into_styled(style)
        .draw(display)
        .unwrap();
    }
}

fn draw_food(display: &mut Display, game: &Game) {
    let x = game.food.x * GRID_SIZE;
    let y = game.food.y * GRID_SIZE;
    Rectangle::new(
        Point::new(x, y),
        Size::new(GRID_SIZE as u32 - 1, GRID_SIZE as u32 - 1),
    )
    .into_styled(PrimitiveStyle::with_fill(FOOD_COLOR))
    .draw(display)
    .unwrap();
}

fn draw_hud(display: &mut Display, score: u16) {
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let mut buf = [0u8; 16];
    let score_str = format_u16(score, &mut buf);
    Text::new(score_str, Point::new(4, 10), style)
        .draw(display)
        .unwrap();
}

fn draw_frame(display: &mut Display, game: &Game) {
    // Clear and redraw everything (simpler approach for snake)
    Rectangle::new(Point::zero(), Size::new(W as u32, H as u32))
        .into_styled(BLACK)
        .draw(display)
        .unwrap();

    draw_snake(display, game);
    draw_food(display, game);
    draw_hud(display, game.score);
}

fn draw_title(display: &mut Display) {
    Rectangle::new(Point::zero(), Size::new(W as u32, H as u32))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .unwrap();

    let big = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_YELLOW);
    let small = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);

    Text::new("SNAKE", Point::new(W / 2 - 15, H / 2 - 20), big)
        .draw(display)
        .unwrap();
    Text::new("D-pad to move", Point::new(W / 2 - 42, H / 2 - 5), small)
        .draw(display)
        .unwrap();
    Text::new("Press A to start", Point::new(W / 2 - 48, H / 2 + 10), small)
        .draw(display)
        .unwrap();
}

fn draw_game_over(display: &mut Display, score: u16) {
    Rectangle::new(Point::zero(), Size::new(W as u32, H as u32))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .unwrap();

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);
    let small = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);

    Text::new("GAME OVER", Point::new(W / 2 - 36, H / 2 - 15), style)
        .draw(display)
        .unwrap();

    let mut buf = [0u8; 24];
    let score_str = format_score(score, &mut buf);
    Text::new(score_str, Point::new(W / 2 - 36, H / 2 + 0), small)
        .draw(display)
        .unwrap();

    Text::new("Press A to restart", Point::new(W / 2 - 54, H / 2 + 20), small)
        .draw(display)
        .unwrap();
}

fn format_u16(mut n: u16, buf: &mut [u8; 16]) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return unsafe { core::str::from_utf8_unchecked(&buf[..1]) };
    }
    let mut i = 0;
    let mut tmp = [0u8; 5];
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    unsafe { core::str::from_utf8_unchecked(&buf[..i]) }
}

fn format_score(score: u16, buf: &mut [u8; 24]) -> &str {
    let prefix = b"Score: ";
    buf[..prefix.len()].copy_from_slice(prefix);
    let mut num_buf = [0u8; 16];
    let num_str = format_u16(score, &mut num_buf);
    let num_bytes = num_str.as_bytes();
    buf[prefix.len()..prefix.len() + num_bytes.len()].copy_from_slice(num_bytes);
    let total = prefix.len() + num_bytes.len();
    unsafe { core::str::from_utf8_unchecked(&buf[..total]) }
}

fn update_leds(leds: &mut Leds, game: &Game) {
    if game.game_over {
        leds.fill(Srgb::new(20, 0, 0));
    } else {
        // Show score as LED bar graph
        let lit = (game.score as usize).min(BAR_COUNT);
        let mut left = [Srgb::new(0u8, 0, 0); BAR_COUNT];
        let mut right = [Srgb::new(0u8, 0, 0); BAR_COUNT];

        for i in 0..lit {
            let color = Srgb::new(0, 10, 0);
            if i < BAR_COUNT / 2 {
                left[i] = color;
            } else {
                right[i - BAR_COUNT / 2] = color;
            }
        }
        leds.set_left_bar(&left);
        leds.set_right_bar(&right);
    }
}

#[embassy_executor::task]
async fn game_task(
    display: &'static mut Display<'static>,
    backlight: &'static mut Backlight,
    leds: &'static mut Leds<'static>,
    buttons: &'static mut Buttons,
) {
    info!("Snake game task started");
    backlight.on();

    loop {
        // Title screen
        draw_title(display);
        leds.clear();
        leds.update().await;

        // Wait for A press
        Buttons::debounce_press(&mut buttons.a).await;

        // Game loop
        let mut game = Game::new();
        draw_initial(display, &game);
        let tick = Duration::from_millis(TICK_MS);

        loop {
            // Poll d-pad for next direction
            if buttons.up.is_low() {
                game.next_direction = Direction::Up;
            } else if buttons.down.is_low() {
                game.next_direction = Direction::Down;
            } else if buttons.left.is_low() {
                game.next_direction = Direction::Left;
            } else if buttons.right.is_low() {
                game.next_direction = Direction::Right;
            }

            game.tick();
            draw_frame(display, &game);
            update_leds(leds, &game);
            leds.update().await;

            if game.game_over {
                Timer::after(Duration::from_millis(500)).await;
                draw_game_over(display, game.score);

                // Flash LEDs for game over
                for _ in 0..3 {
                    leds.fill(Srgb::new(20, 0, 0));
                    leds.update().await;
                    Timer::after(Duration::from_millis(300)).await;
                    leds.clear();
                    leds.update().await;
                    Timer::after(Duration::from_millis(300)).await;
                }

                // Wait for restart
                Buttons::debounce_press(&mut buttons.a).await;
                break; // Restart outer loop
            }

            Timer::after(tick).await;
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = disobey2026badge::init();
    let resources = split_resources!(peripherals);

    esp_alloc::heap_allocator!(size: 128 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    let display = mk_static!(Display<'static>, resources.display.into());
    let backlight = mk_static!(Backlight, resources.backlight.into());
    let leds = mk_static!(Leds<'static>, resources.leds.into());
    let buttons = mk_static!(Buttons, resources.buttons.into());

    spawner.must_spawn(game_task(display, backlight, leds, buttons));

    loop {
        Timer::after(Duration::from_secs(600)).await;
    }
}
