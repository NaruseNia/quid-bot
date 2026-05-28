pub mod alarm;
pub mod ask;
pub mod diary;
pub mod habit;
pub mod help;
pub mod news;
pub mod pomo;
pub mod sleep;
pub mod remind;
pub mod settings;
pub mod today;
pub mod todo;

use crate::Data;

type Error = crate::error::Error;
type Context<'a> = poise::Context<'a, Data, Error>;
