// yatws/src/scan_parameters.rs
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Columns {
  #[serde(rename = "ColumnSetRef", default, skip_serializing_if = "Vec::is_empty")]
  pub column_set_refs: Vec<ColumnSetRef>,

  #[serde(rename = "Column", default, skip_serializing_if = "Vec::is_empty")]
  pub columns: Vec<Column>,

  #[serde(rename = "@varName")]
  pub var_name: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct FilterList {
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
pub struct RangeFilter {
  #[serde(rename = "id")]
  pub id: String,

  #[serde(rename = "category")]
  pub category: String,

  #[serde(rename = "histogram")]
  pub histogram: String,

  #[serde(rename = "access")]
  pub access: String,

  #[serde(rename = "Columns")]
  pub columns: Columns,

  #[serde(rename = "AbstractField", default, skip_serializing_if = "Vec::is_empty")]
  pub abstract_fields: Vec<AbstractField>,

  #[serde(rename = "skipValidation")]
  pub skip_validation: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimpleFilter {
  #[serde(rename = "id")]
  pub id: String,

  #[serde(rename = "category")]
  pub category: String,

  #[serde(rename = "histogram")]
  pub histogram: String,

  #[serde(rename = "abbrev")]
  pub abbrev: Option<String>,

  #[serde(rename = "access")]
  pub access: String,

  #[serde(rename = "Columns")]
  pub columns: Columns,

  #[serde(rename = "AbstractField", default, skip_serializing_if = "Vec::is_empty")]
  pub abstract_fields: Vec<AbstractField>,

  #[serde(rename = "skipValidation")]
  pub skip_validation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TripleComboFilter {
  #[serde(rename = "id")]
  pub id: String,

  #[serde(rename = "category")]
  pub category: String,

  #[serde(rename = "access")]
  pub access: String,

  #[serde(rename = "Columns")]
  pub columns: Option<Columns>,

  #[serde(rename = "TripleComboField")]
  pub triple_combo_field: Option<TripleComboField>,

  #[serde(rename = "AbstractField", default, skip_serializing_if = "Vec::is_empty")]
  pub abstract_fields: Vec<AbstractField>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AbstractField {
  #[serde(rename = "code")]
  pub code: Option<String>,

  #[serde(rename = "codeNot")]
  pub code_not: Option<String>,

  #[serde(rename = "displayName")]
  pub display_name: Option<String>,

  #[serde(rename = "varName")]
  pub var_name: Option<String>,

  #[serde(rename = "dontAllowClearAll")]
  pub dont_allow_clear_all: String,

  #[serde(rename = "abbrev")]
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

  #[serde(rename = "fraction")]
  pub fraction: Option<String>,

  #[serde(rename = "@type")]
  pub type_: Option<String>,

  #[serde(rename = "@varName")]
  pub attr_var_name: Option<String>,
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
  pub display_name: String,

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
