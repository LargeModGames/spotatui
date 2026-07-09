use crate::core::user_config::ColumnSpec;

use super::tables::ColumnId;
use super::util::get_percentage_width;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TableColumnSet {
  Songs,
  AlbumTracks,
  Albums,
  Podcasts,
  Episodes,
  RecentlyPlayed,
}

#[derive(Clone, Copy, PartialEq)]
enum WidthSpec {
  Fixed(u16),
  Percent(f32),
  PercentMinus(f32, u16),
}

#[derive(Clone, Copy)]
struct ColumnDef {
  id: &'static str,
  header: &'static str,
  width: WidthSpec,
  column_id: ColumnId,
}

#[derive(Clone)]
pub struct ResolvedColumn {
  pub id: String,
  pub header: String,
  pub width: u16,
  pub column_id: ColumnId,
}

pub fn resolve_columns(
  table: TableColumnSet,
  layout_width: u16,
  configured: &[ColumnSpec],
) -> Vec<ResolvedColumn> {
  let specs = if configured.is_empty() {
    default_specs(table)
  } else {
    configured.to_vec()
  };

  specs
    .iter()
    .filter_map(|spec| {
      let def = column_def(table, &spec.id)?;
      Some(ResolvedColumn {
        id: spec.id.clone(),
        header: spec
          .header
          .clone()
          .unwrap_or_else(|| def.header.to_string()),
        width: spec.width.unwrap_or_else(|| {
          spec.width_percent.map_or_else(
            || resolve_width(layout_width, def.width),
            |pct| get_percentage_width(layout_width, pct / 100.0),
          )
        }),
        column_id: def.column_id,
      })
    })
    .collect()
}

fn default_specs(table: TableColumnSet) -> Vec<ColumnSpec> {
  default_column_ids(table)
    .iter()
    .map(|id| ColumnSpec {
      id: (*id).to_string(),
      ..Default::default()
    })
    .collect()
}

fn default_column_ids(table: TableColumnSet) -> &'static [&'static str] {
  match table {
    TableColumnSet::Songs => &["liked", "title", "artist", "album", "length"],
    TableColumnSet::AlbumTracks => &["liked", "index", "title", "artist", "length"],
    TableColumnSet::Albums => &["title", "artist", "date"],
    TableColumnSet::Podcasts => &["title", "publisher"],
    TableColumnSet::Episodes => &["played", "date", "title", "duration"],
    TableColumnSet::RecentlyPlayed => &["liked", "title", "artist", "length"],
  }
}

fn resolve_width(layout_width: u16, spec: WidthSpec) -> u16 {
  match spec {
    WidthSpec::Fixed(width) => width,
    WidthSpec::Percent(percent) => get_percentage_width(layout_width, percent),
    WidthSpec::PercentMinus(percent, minus) => {
      get_percentage_width(layout_width, percent).saturating_sub(minus)
    }
  }
}

fn column_def(table: TableColumnSet, id: &str) -> Option<ColumnDef> {
  column_defs(table).iter().find(|def| def.id == id).copied()
}

fn column_defs(table: TableColumnSet) -> &'static [ColumnDef] {
  match table {
    TableColumnSet::Songs => &SONG_COLUMNS,
    TableColumnSet::AlbumTracks => &ALBUM_TRACK_COLUMNS,
    TableColumnSet::Albums => &ALBUM_COLUMNS,
    TableColumnSet::Podcasts => &PODCAST_COLUMNS,
    TableColumnSet::Episodes => &EPISODE_COLUMNS,
    TableColumnSet::RecentlyPlayed => &RECENTLY_PLAYED_COLUMNS,
  }
}

const SONG_COLUMNS: [ColumnDef; 6] = [
  liked_column(),
  index_column(),
  title_column(WidthSpec::Percent(0.3), "Title"),
  artist_column(WidthSpec::Percent(0.3)),
  album_column(WidthSpec::Percent(0.3)),
  length_column(WidthSpec::Percent(0.1), "Length"),
];

const ALBUM_TRACK_COLUMNS: [ColumnDef; 6] = [
  liked_column(),
  index_column(),
  title_column(WidthSpec::PercentMinus(2.0 / 5.0, 5), "Title"),
  artist_column(WidthSpec::Percent(2.0 / 5.0)),
  album_column(WidthSpec::Percent(0.3)),
  length_column(WidthSpec::Percent(1.0 / 5.0), "Length"),
];

const ALBUM_COLUMNS: [ColumnDef; 4] = [
  title_column(WidthSpec::Percent(2.0 / 5.0), "Name"),
  artist_column(WidthSpec::Percent(2.0 / 5.0)),
  ColumnDef {
    id: "date",
    header: "Release Date",
    width: WidthSpec::Percent(1.0 / 5.0),
    column_id: ColumnId::None,
  },
  liked_column(),
];

const PODCAST_COLUMNS: [ColumnDef; 2] = [
  title_column(WidthSpec::Percent(2.0 / 5.0), "Name"),
  ColumnDef {
    id: "publisher",
    header: "Publisher(s)",
    width: WidthSpec::Percent(2.0 / 5.0),
    column_id: ColumnId::None,
  },
];

const EPISODE_COLUMNS: [ColumnDef; 4] = [
  ColumnDef {
    id: "played",
    header: "",
    width: WidthSpec::Fixed(2),
    column_id: ColumnId::None,
  },
  ColumnDef {
    id: "date",
    header: "Date",
    width: WidthSpec::PercentMinus(0.5 / 5.0, 2),
    column_id: ColumnId::None,
  },
  title_column(WidthSpec::Percent(3.5 / 5.0), "Name"),
  ColumnDef {
    id: "duration",
    header: "Duration",
    width: WidthSpec::Percent(1.0 / 5.0),
    column_id: ColumnId::None,
  },
];

const RECENTLY_PLAYED_COLUMNS: [ColumnDef; 6] = [
  liked_column(),
  index_column(),
  title_column(WidthSpec::PercentMinus(2.0 / 5.0, 2), "Title"),
  artist_column(WidthSpec::Percent(2.0 / 5.0)),
  album_column(WidthSpec::Percent(0.3)),
  length_column(WidthSpec::Percent(1.0 / 5.0), "Length"),
];

const fn liked_column() -> ColumnDef {
  ColumnDef {
    id: "liked",
    header: "",
    width: WidthSpec::Fixed(2),
    column_id: ColumnId::Liked,
  }
}

const fn index_column() -> ColumnDef {
  ColumnDef {
    id: "index",
    header: "#",
    width: WidthSpec::Fixed(3),
    column_id: ColumnId::None,
  }
}

const fn title_column(width: WidthSpec, header: &'static str) -> ColumnDef {
  ColumnDef {
    id: "title",
    header,
    width,
    column_id: ColumnId::Title,
  }
}

const fn artist_column(width: WidthSpec) -> ColumnDef {
  ColumnDef {
    id: "artist",
    header: "Artist",
    width,
    column_id: ColumnId::None,
  }
}

const fn album_column(width: WidthSpec) -> ColumnDef {
  ColumnDef {
    id: "album",
    header: "Album",
    width,
    column_id: ColumnId::None,
  }
}

const fn length_column(width: WidthSpec, header: &'static str) -> ColumnDef {
  ColumnDef {
    id: "length",
    header,
    width,
    column_id: ColumnId::None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn defaults_are_rendered_when_config_is_empty() {
    let columns = resolve_columns(TableColumnSet::Songs, 100, &[]);
    assert_eq!(
      columns.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
      ["liked", "title", "artist", "album", "length"]
    );
  }

  #[test]
  fn configured_columns_can_reorder_and_override_headers() {
    let columns = resolve_columns(
      TableColumnSet::Songs,
      100,
      &[
        ColumnSpec {
          id: "artist".to_string(),
          header: Some("Band".to_string()),
          width_percent: Some(30.0),
          width: None,
        },
        ColumnSpec {
          id: "title".to_string(),
          header: None,
          width_percent: None,
          width: Some(40),
        },
      ],
    );
    assert_eq!(columns[0].id, "artist");
    assert_eq!(columns[0].header, "Band");
    assert_eq!(columns[0].width, 29);
    assert_eq!(columns[1].id, "title");
    assert_eq!(columns[1].width, 40);
    assert_eq!(columns[1].column_id, ColumnId::Title);
  }
}
