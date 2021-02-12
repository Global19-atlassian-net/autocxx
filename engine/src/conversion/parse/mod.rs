// Copyright 2020 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod bridge_name_tracker;
mod overload_tracker;
pub(crate) mod parse_bindgen;
mod parse_foreign_mod;
mod rust_name_tracker;
pub(crate) mod type_converter;
pub(crate) mod unqualify; // TODO move

pub(crate) use parse_bindgen::ParseBindgen;
