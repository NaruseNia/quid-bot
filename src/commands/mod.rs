pub mod alarm;
pub mod ask;
pub mod diary;
pub mod habit;
pub mod pomo;
pub mod remind;
pub mod todo;

use crate::Data;

type Error = crate::error::Error;
type Context<'a> = poise::Context<'a, Data, Error>;
