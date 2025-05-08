// yatws/src/scan_parameters.rs
use serde::{Deserialize, Deserializer, Serialize};

// Added ColumnItem enum to handle mixed content in Columns
#[derive(Debug, Serialize, Deserialize)]
enum ColumnItem {
  #[serde(rename = "Column")]
  Column(Column),
  #[serde(rename = "ColumnSetRef")]
  ColumnSetRef(ColumnSetRef),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MainScreenDefaultTickers {
  #[serde(rename = "DefaultTickersScans")]
  pub default_tickers_scans: DefaultTickersScans,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultTickersScans {
  #[serde(rename = "DefaultTickersScan", default, skip_serializing_if = "Vec::is_empty")]
  pub default_tickers_scans: Vec<DefaultTickersScan>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultTickersScan {
  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "scanCode")]
  pub scan_code: Option<String>,

  #[serde(rename = "Filter")]
  pub filter: Option<DefaultTickersFilter>,

  #[serde(rename = "instrumentType")]
  pub instrument_type: String,

  #[serde(rename = "delayed")]
  pub delayed: Option<String>,

  #[serde(rename = "secType")]
  pub sec_type: String,

  #[serde(rename = "maxItems")]
  pub max_items: i8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultTickersFilter {
  #[serde(rename = "priceBelow")]
  pub price_below: i16,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnSets {
  #[serde(rename = "ColumnSet", default, skip_serializing_if = "Vec::is_empty")]
  pub column_sets: Vec<ColumnSet>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnSet {
  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "Columns")]
  pub columns: Columns,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SidecarScannerDefaults {
  #[serde(rename = "DefaultSorts")]
  pub default_sorts: DefaultSorts,

  #[serde(rename = "SidecarScannerTemplateList")]
  pub sidecar_scanner_template_list: SidecarScannerTemplateList,

  #[serde(rename = "ScannerProductTypeList")]
  pub scanner_product_type_list: ScannerProductTypeList,

  #[serde(rename = "FieldsConfigurationList")]
  pub fields_configuration_list: FieldsConfigurationList,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultSorts {
  #[serde(rename = "DefaultSort")]
  pub default_sort: DefaultSort,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultSort {
  #[serde(rename = "instruments")]
  pub instruments: String,

  #[serde(rename = "scanCodes")]
  pub scan_codes: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SidecarScannerTemplateList {
  #[serde(rename = "SidecarScannerTemplate", default, skip_serializing_if = "Vec::is_empty")]
  pub sidecar_scanner_templates: Vec<SidecarScannerTemplate>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SidecarScannerTemplate {
  #[serde(rename = "id")]
  pub id: i8,

  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "description")]
  pub description: String,

  #[serde(rename = "AdvancedFilter")]
  pub advanced_filter: Option<AdvancedFilter>,

  #[serde(rename = "product")]
  pub product: Option<String>,

  #[serde(rename = "vendor")]
  pub vendor: Option<String>,

  #[serde(rename = "ArString", default, skip_serializing_if = "Vec::is_empty")]
  pub ar_strings: Vec<ArString>,

  #[serde(rename = "Columns")]
  pub columns: Columns,

  #[serde(rename = "predefined")]
  pub predefined: String,

  #[serde(rename = "sortReversed")]
  pub sort_reversed: String,

  #[serde(rename = "leftChartDividerLocation")]
  pub left_chart_divider_location: i8,

  #[serde(rename = "rightChartDividerLocation")]
  pub right_chart_divider_location: i8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArString {
  #[serde(rename = "String", default, skip_serializing_if = "Vec::is_empty")]
  pub strings: Vec<String>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScannerProductTypeList {
  #[serde(rename = "ScannerProductType", default, skip_serializing_if = "Vec::is_empty")]
  pub scanner_product_types: Vec<ScannerProductType>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScannerProductType {
  #[serde(rename = "secType")]
  pub sec_type: String,

  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "tooltip")]
  pub tooltip: String,

  #[serde(rename = "defaultRegion")]
  pub default_region: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FieldsConfigurationList {
  #[serde(rename = "FieldConfiguration", default, skip_serializing_if = "Vec::is_empty")]
  pub field_configurations: Vec<FieldConfiguration>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FieldConfiguration {
  #[serde(rename = "colId")]
  pub col_id: i16,

  #[serde(rename = "deleteForbidden")]
  pub delete_forbidden: Option<String>,

  #[serde(rename = "displaySupported")]
  pub display_supported: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdvancedScannerDefaults {
  #[serde(rename = "InstrumentConfigurations")]
  pub instrument_configurations: Option<InstrumentConfigurations>,

  #[serde(rename = "RangeFilter", default, skip_serializing_if = "Vec::is_empty")]
  pub range_filters: Vec<RangeFilter>,

  #[serde(rename = "SimpleFilter", default, skip_serializing_if = "Vec::is_empty")]
  pub simple_filters: Vec<SimpleFilter>,

  #[serde(rename = "TripleComboFilter", default, skip_serializing_if = "Vec::is_empty")]
  pub triple_combo_filters: Vec<TripleComboFilter>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstrumentConfigurations {
  #[serde(rename = "InstrumentConfiguration")]
  pub instrument_configuration: InstrumentConfiguration,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstrumentConfiguration {
  #[serde(rename = "instrumentType")]
  pub instrument_type: String,

  #[serde(rename = "scanCode")]
  pub scan_code: String,

  #[serde(rename = "AdvancedFilter")]
  pub advanced_filter: AdvancedFilter,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "ScanParameterResponse")]
pub struct ScanParameterResponse {
  #[serde(rename = "InstrumentList", default, skip_serializing_if = "Vec::is_empty")]
  pub instrument_lists: Vec<InstrumentList>,

  #[serde(rename = "LocationTree")]
  pub location_tree: Option<LocationTree>,

  #[serde(rename = "ScanTypeList")]
  pub scan_type_list: Option<ScanTypeList>,

  #[serde(rename = "SettingList")]
  pub setting_list: Option<SettingList>,

  #[serde(rename = "FilterList")]
  pub filter_list: Option<FilterList>,

  #[serde(rename = "ScannerLayoutList")]
  pub scanner_layout_list: Option<ScannerLayoutList>,

  #[serde(rename = "InstrumentGroupList")]
  pub instrument_group_list: Option<InstrumentGroupList>,

  #[serde(rename = "SimilarProductsDefaults")]
  pub similar_products_defaults: Option<SimilarProductsDefaults>,

  #[serde(rename = "MainScreenDefaultTickers")]
  pub main_screen_default_tickers: Option<MainScreenDefaultTickers>,

  #[serde(rename = "ColumnSets")]
  pub column_sets: Option<ColumnSets>,

  #[serde(rename = "SidecarScannerDefaults")]
  pub sidecar_scanner_defaults: Option<SidecarScannerDefaults>,

  #[serde(rename = "AdvancedScannerDefaults")]
  pub advanced_scanner_defaults: Option<AdvancedScannerDefaults>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstrumentList {
  #[serde(rename = "Instrument", default, skip_serializing_if = "Vec::is_empty")]
  pub instruments: Vec<Instrument>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Instrument {
  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "type")]
  pub type_: String,

  #[serde(rename = "secType")]
  pub sec_type: Option<String>,

  #[serde(rename = "nscanSecType")]
  pub nscan_sec_type: Option<String>,

  #[serde(rename = "filters")]
  pub filters: Option<String>,

  #[serde(rename = "group")]
  pub group: Option<String>,

  #[serde(rename = "shortName")]
  pub short_name: Option<String>,

  #[serde(rename = "cloudScanNotSupported")]
  pub cloud_scan_not_supported: String,

  #[serde(rename = "featureCodes")]
  pub feature_codes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocationTree {
  #[serde(rename = "Location", default, skip_serializing_if = "Vec::is_empty")]
  pub locations: Vec<Location>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Location {
  #[serde(rename = "displayName")]
  pub display_name: String,

  #[serde(rename = "rawPriceOnly")]
  pub raw_price_only: Option<String>,

  #[serde(rename = "locationCode")]
  pub location_code: String,

  #[serde(rename = "instruments")]
  pub instruments: String,

  #[serde(rename = "routeExchange")]
  pub route_exchange: String,

  #[serde(rename = "LocationTree")]
  pub location_tree: Option<LocationTree>,

  #[serde(rename = "delayedOnly")]
  pub delayed_only: Option<String>,

  #[serde(rename = "access")]
  pub access: Option<String>,

  #[serde(rename = "comboLegs")]
  pub combo_legs: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanTypeList {
  #[serde(rename = "ScanType", default, skip_serializing_if = "Vec::is_empty")]
  pub scan_types: Vec<ScanType>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanType {
  #[serde(rename = "displayName")]
  pub display_name: Option<String>,

  #[serde(rename = "vendor")]
  pub vendor: Option<String>,

  #[serde(rename = "scanCode")]
  pub scan_code: Option<String>,

  #[serde(rename = "instruments")]
  pub instruments: Option<String>,

  #[serde(rename = "absoluteColumns")]
  pub absolute_columns: Option<String>,

  #[serde(rename = "Columns")]
  pub columns: Option<Columns>,

  #[serde(rename = "settings")]
  pub settings: Option<String>,

  #[serde(rename = "MobileColumns")]
  pub mobile_columns: Option<MobileColumns>,

  #[serde(rename = "supportsSorting")]
  pub supports_sorting: Option<String>,

  #[serde(rename = "locationFilter")]
  pub location_filter: Option<String>,

  #[serde(rename = "respSizeLimit")]
  pub resp_size_limit: Option<i32>,

  #[serde(rename = "snapshotSizeLimit")]
  pub snapshot_size_limit: Option<i32>,

  #[serde(rename = "delayedAvail")]
  pub delayed_avail: Option<String>,

  #[serde(rename = "feature")]
  pub feature: Option<String>,

  #[serde(rename = "searchName")]
  pub search_name: Option<String>,

  #[serde(rename = "searchDefault")]
  pub search_default: Option<String>,

  #[serde(rename = "access")]
  pub access: Option<String>,

  #[serde(rename = "reuters")]
  pub reuters: Option<String>,
}

#[derive(Debug, Serialize)] // Deserialize is now custom
pub struct Columns {
  // This field is populated by the custom Deserialize impl with Vec::new()
  // after its contents are processed into columns and column_set_refs.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  items_internal: Vec<ColumnItem>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,

  // These fields are populated by the custom Deserialize impl.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub column_set_refs: Vec<ColumnSetRef>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub columns: Vec<Column>,
}

impl<'de> Deserialize<'de> for Columns {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    // Helper struct to capture the raw mixed items and varName attribute
    #[derive(Deserialize)]
    struct ColumnsHelper {
      #[serde(default, rename = "$value")]
      items_internal: Vec<ColumnItem>,
      #[serde(rename = "@varName")]
      var_name: Option<String>,
    }

    let helper = ColumnsHelper::deserialize(deserializer)?;

    let mut columns_vec = Vec::new();
    let mut column_set_refs_vec = Vec::new();

    for item in helper.items_internal {
      match item {
        ColumnItem::Column(c) => columns_vec.push(c),
        ColumnItem::ColumnSetRef(csr) => column_set_refs_vec.push(csr),
      }
    }

    Ok(Columns {
      items_internal: Vec::new(), // Content processed, so clear/reset items_internal
      var_name: helper.var_name,
      columns: columns_vec,
      column_set_refs: column_set_refs_vec,
    })
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnSetRef {
  #[serde(rename = "colId")]
  pub col_id: i8,

  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "display")]
  pub display: String,

  #[serde(rename = "section")]
  pub section: Option<String>,

  #[serde(rename = "minTwsBuild")]
  pub min_tws_build: Option<i16>,

  #[serde(rename = "displayType")]
  pub display_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Column {
  #[serde(rename = "colId")]
  pub col_id: i32,

  #[serde(rename = "name")]
  pub name: Option<String>,

  #[serde(rename = "display")]
  pub display: String,

  #[serde(rename = "section")]
  pub section: Option<String>,

  #[serde(rename = "displayType")]
  pub display_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MobileColumns {
  #[serde(rename = "MobileColumn", default, skip_serializing_if = "Vec::is_empty")]
  pub mobile_columns: Vec<MobileColumn>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MobileColumn {
  #[serde(rename = "colId")]
  pub col_id: i32,

  #[serde(rename = "name")]
  pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingList {
  #[serde(rename = "ComboSetting")]
  pub combo_setting: ComboSetting,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComboSetting {
  #[serde(rename = "settingCode")]
  pub setting_code: String,

  #[serde(rename = "displayName")]
  pub display_name: String,

  #[serde(rename = "ValueList")]
  pub value_list: ValueList,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValueList {
  #[serde(rename = "Value", default, skip_serializing_if = "Vec::is_empty")]
  pub values: Vec<Value>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Value {
  #[serde(rename = "valueCode")]
  pub value_code: String,

  #[serde(rename = "displayName")]
  pub display_name: String,
}

// Add before or near RangeFilter definition
#[derive(Debug, Deserialize, Serialize)]
enum RangeFilterItem {
  #[serde(rename = "id")]
  Id(String),
  #[serde(rename = "category")]
  Category(String),
  #[serde(rename = "vendor")]
  Vendor(String),
  #[serde(rename = "ref")]
  Ref(String),
  #[serde(rename = "histogram")]
  Histogram(String),
  #[serde(rename = "abbrev")]
  Abbrev(String),
  #[serde(rename = "access")]
  Access(String),
  #[serde(rename = "volatilityUnits")]
  VolatilityUnits(String),
  #[serde(rename = "minTwsBuild")]
  MinTwsBuild(i64),
  #[serde(rename = "reuters")]
  Reuters(String),
  #[serde(rename = "Columns")]
  Columns(Columns), // Uses the existing Columns struct
  #[serde(rename = "AbstractField")]
  AbstractField(AbstractField),
  #[serde(rename = "skipValidation")]
  SkipValidation(String),
}

// Modify RangeFilter struct definition
#[derive(Debug, Serialize)] // Deserialize will be custom
pub struct RangeFilter {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub category: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub histogram: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub abbrev: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub vendor: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub refstr: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub access: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub volatility_units: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub min_tws_build: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reuters: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub columns: Option<Columns>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub abstract_fields: Vec<AbstractField>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub skip_validation: Option<String>,
}

// Add custom Deserialize implementation for RangeFilter
impl<'de> Deserialize<'de> for RangeFilter {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    #[derive(Deserialize)]
    struct RangeFilterHelper {
      #[serde(default, rename = "$value")]
      items: Vec<RangeFilterItem>,
      // No attributes on <RangeFilter> tag itself
    }

    let helper = RangeFilterHelper::deserialize(deserializer)?;

    let mut id = None;
    let mut category = None;
    let mut histogram = None;
    let mut vendor = None;
    let mut refstr = None;
    let mut abbrev = None;
    let mut access = None;
    let mut volatility_units = None;
    let mut min_tws_build = None;
    let mut reuters = None;
    let mut columns_data: Option<Columns> = None;
    let mut abstract_fields = Vec::new();
    let mut skip_validation = None;

    for item in helper.items {
      match item {
        RangeFilterItem::Id(s) => id = Some(s),
        RangeFilterItem::Category(s) => category = Some(s),
        RangeFilterItem::Histogram(s) => histogram = Some(s),
        RangeFilterItem::Vendor(s) => vendor = Some(s),
        RangeFilterItem::Ref(s) => refstr = Some(s),
        RangeFilterItem::Abbrev(s) => abbrev = Some(s),
        RangeFilterItem::Access(s) => access = Some(s),
        RangeFilterItem::VolatilityUnits(s) => volatility_units = Some(s),
        RangeFilterItem::MinTwsBuild(i) => min_tws_build = Some(i),
        RangeFilterItem::Reuters(s) => reuters = Some(s),
        RangeFilterItem::Columns(c) => columns_data = Some(c),
        RangeFilterItem::AbstractField(af) => abstract_fields.push(af),
        RangeFilterItem::SkipValidation(s) => skip_validation = Some(s),
      }
    }

    Ok(RangeFilter {
      id,
      category,
      histogram,
      vendor,
      refstr,
      abbrev,
      access,
      volatility_units,
      min_tws_build,
      reuters,
      columns: columns_data,
      abstract_fields,
      skip_validation,
    })
  }
}

// Add before or near SimpleFilter definition
#[derive(Debug, Deserialize, Serialize)]
enum SimpleFilterItem {
  #[serde(rename = "id")]
  Id(String),
  #[serde(rename = "category")]
  Category(String),
  #[serde(rename = "histogram")]
  Histogram(String),
  #[serde(rename = "abbrev")]
  Abbrev(String), // abbrev is Option<String> in SimpleFilter, so this is fine
  #[serde(rename = "access")]
  Access(String),
  #[serde(rename = "Columns")]
  Columns(Columns),
  #[serde(rename = "AbstractField")]
  AbstractField(AbstractField),
  #[serde(rename = "skipValidation")]
  SkipValidation(String), // skip_validation is Option<String> in SimpleFilter
}

#[derive(Debug, Serialize)] // Deserialize will be custom
pub struct SimpleFilter {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub category: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub histogram: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub abbrev: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub access: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub columns: Option<Columns>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub abstract_fields: Vec<AbstractField>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub skip_validation: Option<String>,
}

// Add custom Deserialize implementation for SimpleFilter
impl<'de> Deserialize<'de> for SimpleFilter {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    #[derive(Deserialize)]
    struct SimpleFilterHelper {
      #[serde(default, rename = "$value")]
      items: Vec<SimpleFilterItem>,
    }

    let helper = SimpleFilterHelper::deserialize(deserializer)?;

    let mut id = None;
    let mut category = None;
    let mut histogram = None;
    let mut abbrev = None;
    let mut access = None;
    let mut columns_data: Option<Columns> = None;
    let mut abstract_fields = Vec::new();
    let mut skip_validation = None;

    for item in helper.items {
      match item {
        SimpleFilterItem::Id(s) => id = Some(s),
        SimpleFilterItem::Category(s) => category = Some(s),
        SimpleFilterItem::Histogram(s) => histogram = Some(s),
        SimpleFilterItem::Abbrev(s) => abbrev = Some(s),
        SimpleFilterItem::Access(s) => access = Some(s),
        SimpleFilterItem::Columns(c) => columns_data = Some(c),
        SimpleFilterItem::AbstractField(af) => abstract_fields.push(af),
        SimpleFilterItem::SkipValidation(s) => skip_validation = Some(s),
      }
    }

    Ok(SimpleFilter {
      id,
      category,
      histogram,
      abbrev,
      access,
      columns: columns_data,
      abstract_fields,
      skip_validation,
    })
  }
}

#[derive(Debug, Serialize)] // Deserialize will be custom
pub struct TripleComboFilter {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub category: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub access: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub columns: Option<Columns>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub triple_combo_field: Option<TripleComboField>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub abstract_fields: Vec<AbstractField>,
}

// Add custom Deserialize implementation for TripleComboFilter
impl<'de> Deserialize<'de> for TripleComboFilter {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    #[derive(Deserialize)]
    struct TripleComboFilterHelper {
      #[serde(default, rename = "$value")]
      items: Vec<TripleComboFilterItem>,
    }

    let helper = TripleComboFilterHelper::deserialize(deserializer)?;

    let mut id = None;
    let mut category = None;
    let mut access = None;
    let mut columns_data: Option<Columns> = None;
    let mut triple_combo_field_data: Option<TripleComboField> = None;
    let mut abstract_fields = Vec::new();

    for item in helper.items {
      match item {
        TripleComboFilterItem::Id(s) => id = Some(s),
        TripleComboFilterItem::Category(s) => category = Some(s),
        TripleComboFilterItem::Access(s) => access = Some(s),
        TripleComboFilterItem::Columns(c) => columns_data = Some(c),
        TripleComboFilterItem::TripleComboField(tcf) => triple_combo_field_data = Some(tcf),
        TripleComboFilterItem::AbstractField(af) => abstract_fields.push(af),
      }
    }

    Ok(TripleComboFilter {
      id,
      category,
      access,
      columns: columns_data,
      triple_combo_field: triple_combo_field_data,
      abstract_fields,
    })
  }
}

// Add before or near FilterList definition
#[derive(Debug, Deserialize, Serialize)]
enum FilterListItem {
  #[serde(rename = "RangeFilter")]
  RangeFilter(RangeFilter), // These will use their own custom Deserialize
  #[serde(rename = "SimpleFilter")]
  SimpleFilter(SimpleFilter),
  #[serde(rename = "TripleComboFilter")]
  TripleComboFilter(TripleComboFilter),
  #[serde(rename = "VirtualFilter")] // New variant
  VirtualFilter(VirtualFilter),
}

// Modify FilterList struct definition
#[derive(Debug, Serialize)] // Deserialize will be custom
pub struct FilterList {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub range_filters: Vec<RangeFilter>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub simple_filters: Vec<SimpleFilter>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub triple_combo_filters: Vec<TripleComboFilter>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")] // New field
  pub virtual_filters: Vec<VirtualFilter>,
  #[serde(rename = "@varName", default, skip_serializing_if = "Option::is_none")]
  pub var_name: Option<String>,
}

// Add custom Deserialize implementation for FilterList
impl<'de> Deserialize<'de> for FilterList {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    #[derive(Deserialize)]
    struct FilterListHelper {
      #[serde(default, rename = "$value")]
      items: Vec<FilterListItem>,
      #[serde(rename = "@varName")]
      var_name: Option<String>,
    }

    let helper = FilterListHelper::deserialize(deserializer)?;

    let mut range_filters = Vec::new();
    let mut simple_filters = Vec::new();
    let mut triple_combo_filters = Vec::new();
    let mut virtual_filters = Vec::new(); // New vector

    for item in helper.items {
      match item {
        FilterListItem::RangeFilter(rf) => range_filters.push(rf),
        FilterListItem::SimpleFilter(sf) => simple_filters.push(sf),
        FilterListItem::TripleComboFilter(tcf) => triple_combo_filters.push(tcf),
        FilterListItem::VirtualFilter(vf) => virtual_filters.push(vf), // Handle new variant
      }
    }

    Ok(FilterList {
      range_filters,
      simple_filters,
      triple_combo_filters,
      virtual_filters, // Add to struct initialization
      var_name: helper.var_name,
    })
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilterComponent {
  #[serde(rename = "code")]
  pub code: String,
  #[serde(rename = "ComboValue")]
  pub combo_value: ComboValue,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilterComponents {
  #[serde(rename = "FilterComponent", default, skip_serializing_if = "Vec::is_empty")]
  pub filter_components: Vec<FilterComponent>,
  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VirtualFilter {
  #[serde(rename = "id")]
  pub id: String,
  #[serde(rename = "category")]
  pub category: String,
  #[serde(rename = "displayName", default, skip_serializing_if = "Vec::is_empty")]
  pub display_names: Vec<String>, // Handles multiple displayName elements
  #[serde(rename = "linkGroup")]
  pub link_group: Option<String>,
  #[serde(rename = "FilterComponents")]
  pub filter_components: FilterComponents,
}


// Add before or near TripleComboFilter definition
#[derive(Debug, Deserialize, Serialize)]
enum TripleComboFilterItem {
  #[serde(rename = "id")]
  Id(String),
  #[serde(rename = "category")]
  Category(String),
  #[serde(rename = "access")]
  Access(String),
  #[serde(rename = "Columns")]
  Columns(Columns),
  #[serde(rename = "TripleComboField")]
  TripleComboField(TripleComboField),
  #[serde(rename = "AbstractField")]
  AbstractField(AbstractField),
}

// TripleComboFilter struct definition is already updated by the previous blocks.
// Custom Deserialize implementation for TripleComboFilter is already updated by the previous blocks.

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckBoxTranslator {
  #[serde(rename = "displayName")]
  pub display_name: String,
  #[serde(rename = "tooltip")]
  pub tooltip: String,
  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)] // Added Serialize for completeness
enum AbstractFieldItem {
  #[serde(rename = "code")]
  Code(String),
  #[serde(rename = "codeNot")]
  CodeNot(String),
  #[serde(rename = "displayName")]
  DisplayName(String),
  #[serde(rename = "varName")] // Element <varName>
  VarNameElement(String),
  #[serde(rename = "dontAllowClearAll")]
  DontAllowClearAll(String),
  #[serde(rename = "abbrev")]
  Abbrev(String),
  #[serde(rename = "acceptNegatives")]
  AcceptNegatives(String),
  #[serde(rename = "dontAllowNegative")]
  DontAllowNegative(String),
  #[serde(rename = "skipNotEditableField")]
  SkipNotEditableField(String),
  #[serde(rename = "ComboValues")]
  ComboValues(ComboValues),
  #[serde(rename = "master")]
  Master(String),
  #[serde(rename = "narrowField")]
  NarrowField(String),
  #[serde(rename = "radioButtons")]
  RadioButtons(String),
  #[serde(rename = "suffix")]
  Suffix(String),
  #[serde(rename = "prefix")]
  Prefix(String),
  #[serde(rename = "style")]
  Style(String),
  #[serde(rename = "typeAhead")]
  TypeAhead(String),
  #[serde(rename = "typeAheadList")]
  TypeAheadList(String),
  #[serde(rename = "AbstractField")] // For nested <AbstractField>
  AbstractFieldChild(Box<AbstractField>),
  #[serde(rename = "separator")]
  Separator(String),
  #[serde(rename = "ignoreCase")]
  IgnoreCase(String),
  #[serde(rename = "maxValue")]
  MaxValue(i32),
  #[serde(rename = "minValue")]
  MinValue(i32),
  #[serde(rename = "precision")]
  Precision(i8),
  #[serde(rename = "inverseCheckbox")]
  InverseCheckbox(bool),
  #[serde(rename = "defaultValue")]
  DefaultValue(String),
  #[serde(rename = "linkGroup")]
  LinkGroup(String),
  #[serde(rename = "tooltip")]
  Tooltip(String),
  #[serde(rename = "fraction")]
  Fraction(String),
  #[serde(rename = "CheckBoxTranslator")] // New variant for CheckBoxTranslator
  CheckBoxTranslator(CheckBoxTranslator),
}

#[derive(Debug, Serialize, Default)] // Added Default, Deserialize will be custom
pub struct AbstractField {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub code: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub code_not: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub display_name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub var_name: Option<String>, // For the <varName> element
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dont_allow_clear_all: Option<String>, // Was String, now Option<String>
  #[serde(skip_serializing_if = "Option::is_none")]
  pub abbrev: Option<String>,

  #[serde(rename = "acceptNegatives")]
  pub accept_negatives: Option<String>,

  #[serde(rename = "dontAllowNegative")]
  pub dont_allow_negative: Option<String>,

  #[serde(rename = "skipNotEditableField")]
  pub skip_not_editable_field: Option<String>,

  #[serde(rename = "ComboValues")]
  pub combo_values: Option<ComboValues>,

  #[serde(rename = "master")]
  pub master: Option<String>,

  #[serde(rename = "narrowField")]
  pub narrow_field: Option<String>,

  #[serde(rename = "radioButtons")]
  pub radio_buttons: Option<String>,

  #[serde(rename = "suffix")]
  pub suffix: Option<String>,

  #[serde(rename = "prefix")]
  pub prefix: Option<String>,

  #[serde(rename = "style")]
  pub style: Option<String>,

  #[serde(rename = "typeAhead")]
  pub type_ahead: Option<String>,

  #[serde(rename = "typeAheadList")]
  pub type_ahead_list: Option<String>,

  #[serde(rename = "AbstractField")]
  pub abstract_field: Option<Box<AbstractField>>,

  #[serde(rename = "separator")]
  pub separator: Option<String>,

  #[serde(rename = "ignoreCase")]
  pub ignore_case: Option<String>,

  #[serde(rename = "maxValue")]
  pub max_value: Option<i32>,

  #[serde(rename = "minValue")]
  pub min_value: Option<i32>,

  #[serde(rename = "precision")]
  pub precision: Option<i8>,

  #[serde(rename = "inverseCheckBox")]
  pub inverse_checkbox: Option<bool>,

  #[serde(rename = "defaultValue")]
  pub default_value: Option<String>,

  #[serde(rename = "linkGroup")]
  pub link_group: Option<String>,

  #[serde(rename = "tooltip")]
  pub tooltip: Option<String>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub fraction: Option<String>,

  // New field for CheckBoxTranslator
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub check_box_translators: Vec<CheckBoxTranslator>,

  // Attributes of the <AbstractField> tag itself
  #[serde(rename = "@type", skip_serializing_if = "Option::is_none")]
  pub type_: Option<String>,
  #[serde(rename = "@varName", skip_serializing_if = "Option::is_none")]
  pub attr_var_name: Option<String>, // Attribute @varName
}

impl<'de> Deserialize<'de> for AbstractField {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    // Helper struct to capture all child elements and attributes
    #[derive(Deserialize)]
    struct AbstractFieldHelper {
      #[serde(default, rename = "$value")]
      items: Vec<AbstractFieldItem>, // Captures all child XML elements
      #[serde(rename = "@type")]
      type_: Option<String>,
      #[serde(rename = "@varName")]
      attr_var_name: Option<String>,
    }

    let helper = AbstractFieldHelper::deserialize(deserializer)?;
    let mut abstract_field_instance = AbstractField::default(); // Initialize with default values

    // Assign attributes
    abstract_field_instance.type_ = helper.type_;
    abstract_field_instance.attr_var_name = helper.attr_var_name;

    // Populate fields from the collected items
    for item in helper.items {
      match item {
        AbstractFieldItem::Code(s) => abstract_field_instance.code = Some(s),
        AbstractFieldItem::CodeNot(s) => abstract_field_instance.code_not = Some(s),
        AbstractFieldItem::DisplayName(s) => abstract_field_instance.display_name = Some(s),
        AbstractFieldItem::VarNameElement(s) => abstract_field_instance.var_name = Some(s),
        AbstractFieldItem::DontAllowClearAll(s) => abstract_field_instance.dont_allow_clear_all = Some(s),
        AbstractFieldItem::Abbrev(s) => abstract_field_instance.abbrev = Some(s),
        AbstractFieldItem::AcceptNegatives(s) => abstract_field_instance.accept_negatives = Some(s),
        AbstractFieldItem::DontAllowNegative(s) => abstract_field_instance.dont_allow_negative = Some(s),
        AbstractFieldItem::SkipNotEditableField(s) => abstract_field_instance.skip_not_editable_field = Some(s),
        AbstractFieldItem::ComboValues(cv) => abstract_field_instance.combo_values = Some(cv),
        AbstractFieldItem::Master(s) => abstract_field_instance.master = Some(s),
        AbstractFieldItem::NarrowField(s) => abstract_field_instance.narrow_field = Some(s),
        AbstractFieldItem::Suffix(s) => abstract_field_instance.suffix = Some(s),
        AbstractFieldItem::Prefix(s) => abstract_field_instance.prefix = Some(s),
        AbstractFieldItem::Style(s) => abstract_field_instance.style = Some(s),
        AbstractFieldItem::RadioButtons(s) => abstract_field_instance.radio_buttons = Some(s),
        AbstractFieldItem::TypeAhead(s) => abstract_field_instance.type_ahead = Some(s),
        AbstractFieldItem::TypeAheadList(s) => abstract_field_instance.type_ahead_list = Some(s),
        AbstractFieldItem::AbstractFieldChild(af_child) => abstract_field_instance.abstract_field = Some(af_child),
        AbstractFieldItem::Separator(s) => abstract_field_instance.separator = Some(s),
        AbstractFieldItem::IgnoreCase(s) => abstract_field_instance.ignore_case = Some(s),
        AbstractFieldItem::MaxValue(i) => abstract_field_instance.max_value = Some(i),
        AbstractFieldItem::MinValue(i) => abstract_field_instance.min_value = Some(i),
        AbstractFieldItem::Precision(i) => abstract_field_instance.precision = Some(i),
        AbstractFieldItem::InverseCheckbox(b) => abstract_field_instance.inverse_checkbox = Some(b),
        AbstractFieldItem::DefaultValue(s) => abstract_field_instance.default_value = Some(s),
        AbstractFieldItem::LinkGroup(s) => abstract_field_instance.link_group = Some(s),
        AbstractFieldItem::Tooltip(s) => abstract_field_instance.tooltip = Some(s),
        AbstractFieldItem::Fraction(s) => abstract_field_instance.fraction = Some(s),
        AbstractFieldItem::CheckBoxTranslator(cbt) => abstract_field_instance.check_box_translators.push(cbt),
      }
    }
    Ok(abstract_field_instance)
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TripleComboField {
  #[serde(rename = "code")]
  pub code: String,

  #[serde(rename = "codeNot")]
  pub code_not: String,

  #[serde(rename = "displayName")]
  pub display_name: String,

  #[serde(rename = "dontAllowClearAll")]
  pub dont_allow_clear_all: String,

  #[serde(rename = "separator")]
  pub separator: String,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComboValues {
  #[serde(rename = "ComboValue", default, skip_serializing_if = "Vec::is_empty")]
  pub combo_values: Vec<ComboValue>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComboValue {
  #[serde(rename = "code")]
  pub code: Option<String>,

  #[serde(rename = "displayName")]
  pub display_name: Option<String>, // Changed from String to Option<String>

  #[serde(rename = "tooltip")]
  pub tooltip: Option<String>,

  #[serde(rename = "default")]
  pub default: String,

  #[serde(rename = "syntheticAll")]
  pub synthetic_all: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScannerLayoutList {
  #[serde(rename = "ScannerLayout", default, skip_serializing_if = "Vec::is_empty")]
  pub scanner_layouts: Vec<ScannerLayout>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScannerLayout {
  #[serde(rename = "instrument")]
  pub instrument: String,

  #[serde(rename = "LayoutComponent", default, skip_serializing_if = "Vec::is_empty")]
  pub layout_components: Vec<LayoutComponent>,

  #[serde(rename = "FilterList")]
  pub filter_list: FilterList,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LayoutComponent {
  #[serde(rename = "ScannerParameter")]
  pub scanner_parameter: Option<ScannerParameter>,

  #[serde(rename = "filters")]
  pub filters: Option<String>,

  #[serde(rename = "title")]
  pub title: Option<String>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScannerParameter {
  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstrumentGroupList {
  #[serde(rename = "InstrumentGroup", default, skip_serializing_if = "Vec::is_empty")]
  pub instrument_groups: Vec<InstrumentGroup>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstrumentGroup {
  #[serde(rename = "group")]
  pub group: String,

  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "secType")]
  pub sec_type: String,

  #[serde(rename = "nscanSecType")]
  pub nscan_sec_type: Option<String>,

  #[serde(rename = "routeExchange")]
  pub route_exchange: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimilarProductsDefaults {
  #[serde(rename = "SimilarProductsDefault")]
  pub similar_products_default: SimilarProductsDefault,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimilarProductsDefault {
  #[serde(rename = "instrumentType")]
  pub instrument_type: String,

  #[serde(rename = "CompressedScans")]
  pub compressed_scans: CompressedScans,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompressedScans {
  #[serde(rename = "CompressedScan", default, skip_serializing_if = "Vec::is_empty")]
  pub compressed_scans: Vec<CompressedScan>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompressedScan {
  #[serde(rename = "reuters")]
  pub reuters: Option<String>,

  #[serde(rename = "name")]
  pub name: String,

  #[serde(rename = "scanCode")]
  pub scan_code: String,

  #[serde(rename = "AdvancedFilter")]
  pub advanced_filter: AdvancedFilter,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdvancedFilter {
  #[serde(rename = "MKTCAP")]
  pub mktcap: Option<String>,

  #[serde(rename = "industryLike")]
  pub industry_like: Option<String>,

  #[serde(rename = "usdMarketCapAbove")]
  pub usd_market_cap_above: Option<i32>,

  #[serde(rename = "avgVolumeAbove")]
  pub avg_volume_above: Option<i32>,

  #[serde(rename = "optVolumeAbove")]
  pub opt_volume_above: Option<i16>,

  #[serde(rename = "etfAssetsAbove")]
  pub etf_assets_above: Option<i8>,

  #[serde(rename = "mfDividendYieldWAvgAbove")]
  pub mf_dividend_yield_w_avg_above: Option<i8>,

  #[serde(rename = "mfEPSGrowth5yrAbove")]
  pub mf_eps_growth_5yr_above: Option<i8>,

  #[serde(rename = "mfDividendPayoutRatio5yrBelow")]
  pub mf_dividend_payout_ratio_5yr_below: Option<i8>,

  #[serde(rename = "mfCalculatedAverageQualityAbove")]
  pub mf_calculated_average_quality_above: Option<i8>,

  #[serde(rename = "mfTotalReturnScoreOverallAbove")]
  pub mf_total_return_score_overall_above: Option<i8>,

  #[serde(rename = "mfExpenseScoreOverallAbove")]
  pub mf_expense_score_overall_above: Option<i8>,

  #[serde(rename = "moodyRatingAbove")]
  pub moody_rating_above: Option<String>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
}
