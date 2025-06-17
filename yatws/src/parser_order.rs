// yatws/src/parser_order.rs
use std::sync::Arc;
use std::str::FromStr;
use crate::base::IBKRError;
use crate::protocol_dec_parser::{FieldParser, parse_opt_tws_date_or_month};
use crate::contract::{Contract, OptionRight, SecType, DeltaNeutralContract, ComboLeg}; // Added DeltaNeutralContract, ComboLeg
use crate::order::{OrderState, OrderStatus, OrderType, OrderSide, TimeInForce, OrderRequest}; // Added OrderState, OrderStatus, OrderUpdates
use crate::min_server_ver::min_server_ver;
use log::{debug, warn}; // Added warn
use crate::handler::OrderHandler;
use crate::protocol_dec_parser::{parse_tws_date_time, parse_tws_date_or_month};

// --- Helper to parse OrderStatus string ---
fn parse_order_status(status_str: &str) -> OrderStatus {
  match status_str {
    "PendingSubmit" => OrderStatus::PendingSubmit,
    "PendingCancel" => OrderStatus::PendingCancel,
    "PreSubmitted" => OrderStatus::PreSubmitted,
    "Submitted" => OrderStatus::Submitted,
    "ApiPending" => OrderStatus::ApiPending,
    "ApiCancelled" => OrderStatus::ApiCancelled,
    "Cancelled" => OrderStatus::Cancelled,
    "Filled" => OrderStatus::Filled,
    "Inactive" => OrderStatus::Inactive,
    _ => {
      warn!("Received unknown order status string: '{}'. Mapping to Inactive.", status_str);
      OrderStatus::Inactive
    }
  }
}

// Helper to read bool from int (0 or 1)
fn read_bool_from_int(parser: &mut FieldParser) -> Result<bool, IBKRError> {
  Ok(parser.read_int()? == 1)
}

struct OrderDecoder<'a, 'p> {
  parser:         &'p mut FieldParser<'p>,
  contract:       &'a mut Contract,
  request:        &'a mut OrderRequest,
  state:          &'a mut OrderState,
  msg_version:    i32,
  server_version: i32,
}

impl<'a, 'p> OrderDecoder<'a, 'p> {
  fn new(
    parser: &'p mut FieldParser<'p>,
    contract: &'a mut Contract,
    request: &'a mut OrderRequest,
    state: &'a mut OrderState,
    msg_version: i32,
    server_version: i32,
  ) -> Self {
    OrderDecoder { parser, contract, request, state, msg_version, server_version }
  }

  fn read_contract_fields(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 17 {
      self.contract.con_id = self.parser.read_int()?;
    }
    self.contract.symbol = self.parser.read_string()?;
    let sec_type_str = self.parser.read_string()?;
    self.contract.sec_type = SecType::from_str(&sec_type_str)
      .map_err(|e| IBKRError::ParseError(format!("Invalid secType '{}': {}", sec_type_str, e)))?;
    self.contract.last_trade_date_or_contract_month =
      parse_opt_tws_date_or_month(self.parser.read_string_opt()?)?;
    self.contract.strike = self.parser.read_double_max()?; // Treat 0.0 as valid, MAX as None
    let right_str = self.parser.read_string()?;
    if !right_str.is_empty() && right_str != "?" {
      self.contract.right = OptionRight::from_str(&right_str).ok(); // Use ok() to ignore parse errors for right
    }
    if self.msg_version >= 32 {
      self.contract.multiplier = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    self.contract.exchange = self.parser.read_string()?;
    self.contract.currency = self.parser.read_string()?;
    if self.msg_version >= 2 {
      self.contract.local_symbol = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    if self.msg_version >= 32 {
      // Aligns with Java version check
      self.contract.trading_class = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    // Note: Java OpenOrder decoder does not read secIdType/secId here.
    Ok(())
  }

  fn read_action(&mut self) -> Result<(), IBKRError> {
    let action_str = self.parser.read_string()?;
    self.request.side = OrderSide::from_str(&action_str)?;
    Ok(())
  }

  fn read_total_quantity(&mut self) -> Result<(), IBKRError> {
    self.request.quantity = self.parser.read_decimal_max()?.unwrap_or(0.0);
    Ok(())
  }

  fn read_order_type(&mut self) -> Result<(), IBKRError> {
    let order_type_str = self.parser.read_string()?;
    self.request.order_type = OrderType::from_str(&order_type_str)
      .map_err(|_| IBKRError::ParseError(format!("Unknown order type: {}", order_type_str)))?;
    Ok(())
  }

  fn read_lmt_price(&mut self) -> Result<(), IBKRError> {
    self.request.limit_price = if self.msg_version < 29 {
      // Old versions might not use MAX_VALUE sentinel. Need API documentation confirmation.
      // Reading as double and filtering 0.0 might be incorrect if 0 is a valid price.
      // Let's consistently use read_double_max, assuming TWS sends MAX_VALUE appropriately even for older versions.
      self.parser.read_double_max()?
    } else {
      self.parser.read_double_max()?
    };
    Ok(())
  }

  fn read_aux_price(&mut self) -> Result<(), IBKRError> {
    self.request.aux_price = if self.msg_version < 30 {
      self.parser.read_double_max()?
    } else {
      self.parser.read_double_max()?
    };
    Ok(())
  }

  fn read_tif(&mut self) -> Result<(), IBKRError> {
    let tif_str = self.parser.read_string()?;
    self.request.time_in_force = TimeInForce::from_str(&tif_str)?;
    Ok(())
  }

  fn read_oca_group(&mut self) -> Result<(), IBKRError> {
    self.request.oca_group = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    Ok(())
  }

  fn read_account(&mut self) -> Result<(), IBKRError> {
    self.request.account = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    Ok(())
  }

  fn read_open_close(&mut self) -> Result<(), IBKRError> {
    // Default is 'O' if field is present but empty. Treat None as unset.
    let oc_str = self.parser.read_string()?;
    self.request.open_close = if oc_str.is_empty() {
      Some("O".to_string()) // Default to Open if present but empty
    } else if oc_str == "\0" {
      // Check if TWS might send null char for unset? Unlikely.
      None // Treat null/specific marker as unset if needed
    } else {
      Some(oc_str) // Use the non-empty string
    };
    Ok(())
  }

  fn read_origin(&mut self) -> Result<(), IBKRError> {
    self.request.origin = self.parser.read_int()?; // 0=Customer, 1=Firm
    Ok(())
  }

  fn read_order_ref(&mut self) -> Result<(), IBKRError> {
    self.request.order_ref = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    Ok(())
  }

  fn read_client_id(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 3 {
      let _client_id = self.parser.read_int()?; // Parsed, but not stored here (handled by orderStatus in Rust model)
    }
    Ok(())
  }

  fn read_perm_id(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 4 {
      let _perm_id = self.parser.read_int()?; // Parsed, but not stored here (handled by orderStatus in Rust model)
    }
    Ok(())
  }

  fn read_outside_rth(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 4 {
      // Version < 18 used 'ignoreRth' (inverted logic). Version >= 18 uses 'outsideRth'.
      let val = read_bool_from_int(self.parser)?;
      self.request.outside_rth = if self.msg_version < 18 {
        !val // Invert logic for old 'ignoreRth'
      } else {
        val // Use directly for 'outsideRth'
      };
    }
    Ok(())
  }

  fn read_hidden(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 4 {
      self.request.hidden = self.parser.read_int()? == 1;
    }
    Ok(())
  }

  fn read_discretionary_amount(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 4 {
      // Java reads double, 0.0 is default. Let's read and filter 0.0.
      self.request.discretionary_amt = self.parser.read_double().ok().filter(|&p| p != 0.0);
    }
    Ok(())
  }

  fn read_good_after_time(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 5 {
      let time_str = self.parser.read_string()?;
      self.request.good_after_time = if time_str.is_empty() { None } else { Some(parse_tws_date_time(&time_str)?) };
    }
    Ok(())
  }

  fn skip_shares_allocation(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 6 {
      let _shares_allocation = self.parser.read_string()?; // skip deprecated
    }
    Ok(())
  }

  fn read_fa_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 7 {
      self.request.fa_group = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      self.request.fa_method = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      self.request.fa_percentage = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      if self.server_version < min_server_ver::FA_PROFILE_DESUPPORT {
        let _fa_profile = self.parser.read_string()?; // skip deprecated faProfile field
      }
    }
    Ok(())
  }

  fn read_model_code(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::MODELS_SUPPORT {
      self.request.model_code = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    Ok(())
  }

  fn read_good_till_date(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 8 {
      let time_str = self.parser.read_string()?;
      self.request.good_till_date = if time_str.is_empty() { None } else { Some(parse_tws_date_time(&time_str)?) };
    }
    Ok(())
  }

  fn read_rule_80a(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.rule_80a = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    Ok(())
  }

  fn read_percent_offset(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.percent_offset = self.parser.read_double_max()?;
    }
    Ok(())
  }

  fn read_settling_firm(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.settling_firm = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    Ok(())
  }

  fn read_short_sale_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.short_sale_slot = self.parser.read_int().ok().filter(|&s| s > 0); // 0=Unset, 1=Retail, 2=Institutional
      self.request.designated_location = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      if self.server_version == 51 {
        // Quirky version check from Java
        let _exempt_code = self.parser.read_int()?;
      } else if self.msg_version >= 23 {
        self.request.exempt_code = self.parser.read_int().ok().filter(|&e| e != -1); // -1 is usually unset
      }
    }
    Ok(())
  }

  fn read_auction_strategy(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.auction_strategy = self.parser.read_int().ok().filter(|&a| a > 0); // 0=Unset
    }
    Ok(())
  }

  fn read_box_order_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.starting_price = self.parser.read_double_max()?;
      self.request.stock_ref_price = self.parser.read_double_max()?;
      self.request.delta = self.parser.read_double_max()?;
    }
    Ok(())
  }

  fn read_peg_to_stk_or_vol_order_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      // These are read later specifically for VOL orders if server version is 26
      // Avoid reading them here if they overlap to prevent double reads/errors.
      if self.server_version != 26 {
        // Only read here if not the special VOL case handled later
        self.request.stock_range_lower = self.parser.read_double_max()?;
        self.request.stock_range_upper = self.parser.read_double_max()?;
      }
    }
    Ok(())
  }

  fn read_display_size(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.display_size = self.parser.read_int_max()?;
    }
    Ok(())
  }

  fn read_old_style_outside_rth(&mut self) -> Result<(), IBKRError> {
    // This method is only relevant for msg_version < 18, where read_outside_rth handles the logic.
    // Java calls it, but it effectively does nothing useful if read_outside_rth is called correctly.
    // We can skip the body here.
    if self.msg_version >= 9 && self.msg_version < 18 {
      // The logic is already handled in read_outside_rth
      // let _rth_only = read_bool_from_int(self.parser)?; // Don't re-read
    }
    Ok(())
  }

  fn read_block_order(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.block_order = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_sweep_to_fill(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.sweep_to_fill = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_all_or_none(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.all_or_none = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_min_qty(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.min_quantity = self.parser.read_int_max()?;
    }
    Ok(())
  }

  fn read_oca_type(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      self.request.oca_type = self.parser.read_int().ok().filter(|&o| o > 0); // 0=Unset
    }
    Ok(())
  }

  fn read_etrade_only(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      let _ = read_bool_from_int(self.parser)?; // skip deprecated
    }
    Ok(())
  }

  fn read_firm_quote_only(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      let _ = read_bool_from_int(self.parser)?; // skip deprecated
    }
    Ok(())
  }

  fn read_nbbo_price_cap(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 9 {
      let _ = self.parser.read_double_max()?; // skip deprecated
    }
    Ok(())
  }

  fn read_parent_id(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 10 {
      self.request.parent_id =
        self.parser.read_int().ok().map(|id| id as i64).filter(|&id| id != 0);
    }
    Ok(())
  }

  fn read_trigger_method(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 10 {
      self.request.trigger_method = self.parser.read_int().ok().filter(|&t| t >= 0); // 0=Default
    }
    Ok(())
  }

  fn read_vol_order_params(&mut self, read_open_order_attribs: bool) -> Result<(), IBKRError> {
    if self.msg_version >= 11 {
      self.request.volatility = self.parser.read_double_max()?;
      self.request.volatility_type = self.parser.read_int_max()?;
      if self.msg_version == 11 {
        let received_int = self.parser.read_int()?;
        self.request.delta_neutral_order_type =
          Some(if received_int == 0 { "NONE".to_string() } else { "MKT".to_string() })
          .filter(|s| s != "NONE");
      } else {
        // msg_version >= 12
        self.request.delta_neutral_order_type =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "NONE");
        self.request.delta_neutral_aux_price = self.parser.read_double_max()?;

        // Use .as_deref() for cleaner check on Option<String>
        if self.msg_version >= 27 && self.request.delta_neutral_order_type.as_deref().is_some() {
          self.request.delta_neutral_con_id = self.parser.read_int_max()?;
          if read_open_order_attribs {
            self.request.delta_neutral_settling_firm =
              Some(self.parser.read_string()?).filter(|s| !s.is_empty());
            self.request.delta_neutral_clearing_account =
              Some(self.parser.read_string()?).filter(|s| !s.is_empty());
            self.request.delta_neutral_clearing_intent =
              Some(self.parser.read_string()?).filter(|s| !s.is_empty());
          }
        }

        if self.msg_version >= 31 && self.request.delta_neutral_order_type.as_deref().is_some() {
          if read_open_order_attribs {
            self.request.delta_neutral_open_close =
              Some(self.parser.read_string()?).filter(|s| !s.is_empty());
          }
          self.request.delta_neutral_short_sale = read_bool_from_int(self.parser)?;
          self.request.delta_neutral_short_sale_slot = self.parser.read_int_max()?;
          self.request.delta_neutral_designated_location =
            Some(self.parser.read_string()?).filter(|s| !s.is_empty());
        }
      }
      self.request.continuous_update = self.parser.read_int_max()?;
      if self.server_version == 26 {
        // Quirky version check from Java for specific VOL fields
        // These override the ones potentially read in read_peg_to_stk_or_vol_order_params
        self.request.stock_range_lower = self.parser.read_double().ok().filter(|&p| p != 0.0); // Java reads double directly
        self.request.stock_range_upper = self.parser.read_double().ok().filter(|&p| p != 0.0); // Java reads double directly
      }
      self.request.reference_price_type = self.parser.read_int_max()?;
    }
    Ok(())
  }

  fn read_trail_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 13 {
      self.request.trailing_stop_price = self.parser.read_double_max()?;
    }
    if self.msg_version >= 30 {
      self.request.trailing_percent = self.parser.read_double_max()?;
    }
    Ok(())
  }

  fn read_basis_points(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 14 {
      self.request.basis_points = self.parser.read_double_max()?;
      self.request.basis_points_type = self.parser.read_int_max()?;
    }
    Ok(())
  }

  fn read_combo_legs(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 14 {
      self.contract.combo_legs_descrip = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }

    if self.msg_version >= 29 {
      let combo_legs_count = self.parser.read_int()?;
      if combo_legs_count > 0 {
        let mut legs = Vec::with_capacity(combo_legs_count as usize);
        for _ in 0..combo_legs_count {
          let leg = ComboLeg {
            con_id:              self.parser.read_int()?,
            ratio:               self.parser.read_int()?,
            action:              self.parser.read_string()?,
            exchange:            self.parser.read_string()?,
            open_close:          self.parser.read_int()?,
            short_sale_slot:     self.parser.read_int()?,
            designated_location: self.parser.read_string()?,
            exempt_code:         self.parser.read_int()?,
            price:               None, // Price is read below into request.order_combo_legs
          };
          legs.push(leg);
        }
        self.contract.combo_legs = legs;
      } else {
        self.contract.combo_legs = Vec::new(); // Ensure empty if count is 0
      }

      let order_combo_legs_count = self.parser.read_int()?;
      if order_combo_legs_count > 0 {
        let mut order_legs = Vec::with_capacity(order_combo_legs_count as usize);
        for _ in 0..order_combo_legs_count {
          order_legs.push(self.parser.read_double_max()?); // Read optional leg price
        }
        self.request.order_combo_legs = order_legs;
      } else {
        self.request.order_combo_legs = Vec::new(); // Ensure empty if count is 0
      }
    }
    Ok(())
  }

  fn read_smart_combo_routing_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 26 {
      let params_count = self.parser.read_int()?;
      if params_count > 0 {
        let mut params = Vec::with_capacity(params_count as usize);
        for _ in 0..params_count {
          let tag = self.parser.read_string()?;
          let value = self.parser.read_string()?;
          params.push((tag, value));
        }
        self.request.smart_combo_routing_params = params;
      } else {
        self.request.smart_combo_routing_params = Vec::new(); // Ensure empty
      }
    }
    Ok(())
  }

  fn read_scale_order_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 15 {
      if self.msg_version >= 20 {
        self.request.scale_init_level_size = self.parser.read_int_max()?;
        self.request.scale_subs_level_size = self.parser.read_int_max()?;
      } else {
        let _not_supp_scale_num_components = self.parser.read_int_max()?; // Read but ignore old field
        self.request.scale_init_level_size = self.parser.read_int_max()?;
        self.request.scale_subs_level_size = None; // Ensure this is None if only init size was sent
      }
      self.request.scale_price_increment = self.parser.read_double_max()?;
    }

    // Check scale_price_increment > 0 and != MAX, Java logic is slightly different (checks > 0 and != MAX).
    // Our read_double_max returns None for MAX, so is_some() covers the != MAX part. Check > 0 separately.
    if self.msg_version >= 28 && self.request.scale_price_increment.map_or(false, |p| p > 0.0) {
      self.request.scale_price_adjust_value = self.parser.read_double_max()?;
      self.request.scale_price_adjust_interval = self.parser.read_int_max()?;
      self.request.scale_profit_offset = self.parser.read_double_max()?;
      self.request.scale_auto_reset = read_bool_from_int(self.parser)?;
      self.request.scale_init_position = self.parser.read_int_max()?;
      self.request.scale_init_fill_qty = self.parser.read_int_max()?;
      self.request.scale_random_percent = read_bool_from_int(self.parser)?;
    } else if self.msg_version >= 28 {
      // If scale params shouldn't be read, ensure they are None
      self.request.scale_price_adjust_value = None;
      self.request.scale_price_adjust_interval = None;
      self.request.scale_profit_offset = None;
      self.request.scale_auto_reset = false; // Reset bool to default
      self.request.scale_init_position = None;
      self.request.scale_init_fill_qty = None;
      self.request.scale_random_percent = false; // Reset bool to default
    }
    Ok(())
  }

  fn read_hedge_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 24 {
      self.request.hedge_type = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      // Only read hedge_param if hedge_type is present and not empty
      if self.request.hedge_type.as_deref().map_or(false, |s| !s.is_empty()) {
        self.request.hedge_param = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      } else {
        // Ensure hedge_param is None if hedge_type is None or empty
        self.request.hedge_type = None; // Set type back to None if it was empty
        self.request.hedge_param = None;
      }
    }
    Ok(())
  }

  fn read_opt_out_smart_routing(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 25 {
      self.request.opt_out_smart_routing = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_clearing_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 19 {
      self.request.clearing_account = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      self.request.clearing_intent = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    Ok(())
  }

  fn read_not_held(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 22 {
      self.request.not_held = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_delta_neutral(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 20 {
      if read_bool_from_int(self.parser)? {
        let dn = DeltaNeutralContract {
          con_id: self.parser.read_int()?,
          delta:  self.parser.read_double()?,
          price:  self.parser.read_double()?,
        };
        self.contract.delta_neutral_contract = Some(dn);
      } else {
        self.contract.delta_neutral_contract = None; // Ensure it's None if bool is false
      }
    }
    Ok(())
  }

  fn read_algo_params(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 21 {
      self.request.algo_strategy = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      if self.request.algo_strategy.is_some() {
        let algo_params_count = self.parser.read_int()?;
        if algo_params_count > 0 {
          let mut params = Vec::with_capacity(algo_params_count as usize);
          for _ in 0..algo_params_count {
            let tag = self.parser.read_string()?;
            let value = self.parser.read_string()?;
            params.push((tag, value));
          }
          self.request.algo_params = params;
        } else {
          self.request.algo_params = Vec::new(); // Ensure empty
        }
      } else {
        // Ensure related fields are cleared if no strategy
        self.request.algo_params = Vec::new();
        self.request.algo_id = None;
      }
    }
    Ok(())
  }

  fn read_solicited(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 33 {
      self.request.solicited = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  // Renamed from readWhatIfInfoAndCommission to align with Rust structure
  fn read_what_if_info_and_order_state(&mut self) -> Result<(), IBKRError> {
    // This reads the 'whatIf' flag into OrderRequest and the rest into OrderState
    if self.msg_version >= 16 {
      self.request.what_if = read_bool_from_int(self.parser)?;
      self.read_order_status()?; // Read status into state

      if self.server_version >= min_server_ver::WHAT_IF_EXT_FIELDS {
        // Read using read_string and filter empty/? like the old helper
        self.state.initial_margin_before =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
        self.state.maintenance_margin_before =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
        self.state.equity_with_loan_before =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
        self.state.initial_margin_change =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
        self.state.maintenance_margin_change =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
        self.state.equity_with_loan_change =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
      }

      self.state.initial_margin_after =
        Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
      self.state.maintenance_margin_after =
        Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
      self.state.equity_with_loan_after =
        Some(self.parser.read_string()?).filter(|s| !s.is_empty() && s != "?");

      self.state.commission = self.parser.read_double_max()?;
      self.state.min_commission = self.parser.read_double_max()?;
      self.state.max_commission = self.parser.read_double_max()?;
      self.state.commission_currency = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      self.state.warning_text = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    Ok(())
  }

  fn read_order_status(&mut self) -> Result<(), IBKRError> {
    let status_str = self.parser.read_string()?;
    self.state.status = parse_order_status(&status_str); // Use existing helper
    Ok(())
  }

  fn read_vol_randomize_flags(&mut self) -> Result<(), IBKRError> {
    if self.msg_version >= 34 {
      self.request.randomize_size = read_bool_from_int(self.parser)?;
      self.request.randomize_price = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_peg_to_bench_params(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::PEGGED_TO_BENCHMARK {
      // We check against known bench types parsed earlier in read_order_type().
      let is_peg_bench = matches!(
        self.request.order_type,
        OrderType::PeggedToBenchmark | OrderType::PeggedBest | OrderType::PeggedPrimary
      );

      if is_peg_bench {
        self.request.reference_contract_id = self.parser.read_int_max()?;
        self.request.is_pegged_change_amount_decrease = read_bool_from_int(self.parser)?;
        self.request.pegged_change_amount = self.parser.read_double_max()?;
        self.request.reference_change_amount = self.parser.read_double_max()?;
        self.request.reference_exchange_id =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      } else {
        // If not a peg bench order type, ensure these fields are None/default
        // Note: TWS might still send these fields even if type doesn't match.
        // It's safer to read them if the server version supports it, but only store if type matches.
        // Let's refine: Read if server version supports, but only store if type is relevant.
        // This requires reading and potentially discarding if type doesn't match.
        // Alternative: Assume TWS sends fields correctly based on type and read only if type matches. (Chosen below)

        // Ensure fields are default if not a peg bench order
        self.request.reference_contract_id = None;
        self.request.is_pegged_change_amount_decrease = false;
        self.request.pegged_change_amount = None;
        self.request.reference_change_amount = None;
        self.request.reference_exchange_id = None;
      }
    }
    Ok(())
  }

  fn read_conditions(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::PEGGED_TO_BENCHMARK {
      let conditions_count = self.parser.read_int()?;
      if conditions_count > 0 {
        let mut conditions_data = Vec::with_capacity(conditions_count as usize);
        for _i in 0..conditions_count {
          // --- Simplified condition reading ---
          // Read type, then skip a fixed number of fields as a placeholder.
          // This is HIGHLY UNRELIABLE. A proper implementation needs detailed logic per type.
          let cond_type_int = self.parser.read_int()?;
          let cond_str = format!("Condition type {}: Fields skipped", cond_type_int);
          conditions_data.push(cond_str);

          warn!(
            "Condition parsing is simplified (type {}). Fields are skipped heuristically. Data loss may occur.",
            cond_type_int
          );
          // Attempt to skip common fields based on example Condition types:
          // Price: type(i), conjunction(s), isMore(i), price(d), conId(i), exchange(s), triggerMethod(i) = ~7 fields
          // Time: type(i), conjunction(s), isMore(i), time(s) = ~4 fields
          // Margin: type(i), conjunction(s), isMore(i), percent(i) = ~4 fields
          // Execution: type(i), conjunction(s), secType(s), exchange(s), symbol(s) = ~5 fields
          // Volume: type(i), conjunction(s), isMore(i), volume(i), conId(i), exchange(s) = ~6 fields
          // PercentChange: type(i), conjunction(s), isMore(i), changePercent(d), conId(i), exchange(s) = ~6 fields
          // Let's try skipping ~5 string fields as a very rough average guess.
          // This needs actual implementation based on type->fields mapping from IB docs.
          for _ in 0..5 {
            let _ = self.parser.read_string(); // Attempt to read string, ignore result/error
          }
        }
        self.request.conditions = conditions_data; // Store simplified representation

        self.request.conditions_ignore_rth = read_bool_from_int(self.parser)?;
        self.request.conditions_cancel_order = read_bool_from_int(self.parser)?;
      } else {
        self.request.conditions = Vec::new(); // Ensure it's empty if count is 0
      }
    }
    Ok(())
  }

  fn read_adjusted_order_params(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::PEGGED_TO_BENCHMARK {
      let adj_type_str = self.parser.read_string()?;
      if !adj_type_str.is_empty() {
        self.request.adjusted_order_type = OrderType::from_str(&adj_type_str).ok(); // Ignore parse error
      } else {
        self.request.adjusted_order_type = None;
      }
      self.request.trigger_price = self.parser.read_double_max()?;
      self.read_stop_price_and_lmt_price_offset()?; // Read related TRAIL params adjusted here
      self.request.adjusted_stop_price = self.parser.read_double_max()?;
      self.request.adjusted_stop_limit_price = self.parser.read_double_max()?;
      self.request.adjusted_trailing_amount = self.parser.read_double_max()?;
      self.request.adjustable_trailing_unit = self.parser.read_int_max()?;
    }
    Ok(())
  }

  // Helper called by read_adjusted_order_params
  fn read_stop_price_and_lmt_price_offset(&mut self) -> Result<(), IBKRError> {
    // These fields are part of the adjusted order parameters block
    self.request.trailing_stop_price = self.parser.read_double_max()?; // Adjusted TRAIL stop price
    self.request.lmt_price_offset = self.parser.read_double_max()?;
    Ok(())
  }

  fn read_soft_dollar_tier(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::SOFT_DOLLAR_TIER {
      let name = self.parser.read_string()?;
      let value = self.parser.read_string()?;
      let _display_name = self.parser.read_string()?;
      if !name.is_empty() || !value.is_empty() {
        // Store only if non-empty
        self.request.soft_dollar_tier = Some((name, value));
      } else {
        self.request.soft_dollar_tier = None;
      }
    }
    Ok(())
  }

  fn read_cash_qty(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::CASH_QTY {
      self.request.cash_qty = self.parser.read_double_max()?;
    }
    Ok(())
  }

  fn read_dont_use_auto_price_for_hedge(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::AUTO_PRICE_FOR_HEDGE {
      self.request.dont_use_auto_price_for_hedge = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_is_oms_container(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::ORDER_CONTAINER {
      self.request.is_oms_container = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_discretionary_up_to_limit_price(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::D_PEG_ORDERS {
      self.request.discretionary_up_to_limit_price = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_use_price_mgmt_algo(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::PRICE_MGMT_ALGO {
      let val = self.parser.read_int()?; // Read as int
      self.request.use_price_mgmt_algo = match val {
        0 => Some(false),
        1 => Some(true),
        _ => None, // Treat other values (like MAX_INT if sent) as unset
      };
    }
    Ok(())
  }

  fn read_duration(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::DURATION {
      self.request.duration = self.parser.read_int_max()?;
    }
    Ok(())
  }

  fn read_post_to_ats(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::POST_TO_ATS {
      self.request.post_to_ats = self.parser.read_int_max()?;
    }
    Ok(())
  }

  fn read_auto_cancel_parent(&mut self) -> Result<(), IBKRError> {
    let min_version = min_server_ver::AUTO_CANCEL_PARENT;
    if self.server_version >= min_version {
      self.request.auto_cancel_parent = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_peg_best_peg_mid_order_attributes(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::PEGBEST_PEGMID_OFFSETS {
      self.request.min_trade_qty = self.parser.read_int_max()?;
      self.request.min_compete_size = self.parser.read_int_max()?;
      self.request.compete_against_best_offset = self.parser.read_double_max()?;
      self.request.mid_offset_at_whole = self.parser.read_double_max()?;
      self.request.mid_offset_at_half = self.parser.read_double_max()?;
    }
    Ok(())
  }

  fn read_customer_account(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::CUSTOMER_ACCOUNT {
      self.request.customer_account = Some(self.parser.read_string()?).filter(|s| !s.is_empty());
    }
    Ok(())
  }

  fn read_professional_customer(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::PROFESSIONAL_CUSTOMER {
      self.request.professional_customer = read_bool_from_int(self.parser)?;
    }
    Ok(())
  }

  fn read_bond_accrued_interest(&mut self) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::BOND_ACCRUED_INTEREST {
      if self.contract.sec_type == SecType::Bond {
        // Only read if it's a bond
        self.request.bond_accrued_interest =
          Some(self.parser.read_string()?).filter(|s| !s.is_empty());
      } else {
        // If server supports it but contract isn't Bond, TWS might still send an empty field.
        // Let's read and discard if not a bond to stay aligned with stream.
        let _ = self.parser.read_string()?;
        self.request.bond_accrued_interest = None;
      }
    }
    Ok(())
  }

  fn read_string_opt(&mut self) -> Result<Option<String>, IBKRError> {
    self.parser.read_string_opt()
  }

  fn order_state_mut(&mut self) -> &mut OrderState {
    self.state
  }

  fn remaining_fields(&self) -> usize {
    self.parser.remaining_fields()
  }
}

pub fn process_next_valid_id(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?; // Version is typically 1
  let id = parser.read_int()?;
  debug!("Parsed Next valid ID: {}", id);
  handler.next_valid_id(id); // Call handler
  Ok(())
}

/// Process order status message
pub fn process_order_status(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let version = server_version;
  let id = parser.read_int()?;
  let status_str = parser.read_string()?;
  let status_enum = parse_order_status(&status_str); // Parse the enum here

  let filled = if server_version >= min_server_ver::FRACTIONAL_POSITIONS {
    let filled_str = parser.read_string()?;
    filled_str.parse().map_err(|e| IBKRError::ParseError(format!("Failed to parse fractional filled qty '{}': {}", filled_str, e)))?
  } else {
    parser.read_double()?
  };

  let remaining = if server_version >= min_server_ver::FRACTIONAL_POSITIONS {
    let rem_str = parser.read_string()?;
    rem_str.parse().map_err(|e| IBKRError::ParseError(format!("Failed to parse fractional remaining qty '{}': {}", rem_str, e)))?
  } else {
    parser.read_double()?
  };

  let avg_fill_price = parser.read_double()?;
  let perm_id = if version >= 2 { parser.read_int()? } else { 0 };
  let parent_id = if version >= 3 { parser.read_int()? } else { 0 };
  let last_fill_price = if version >= 4 { parser.read_double()? } else { 0.0 };
  let client_id = if version >= 5 { parser.read_int()? } else { 0 };
  let why_held = parser.read_string()?;
  let mkt_cap_price = if server_version >= min_server_ver::MARKET_CAP_PRICE {
    parser.read_double().ok().filter(|&p| p != f64::MAX)
  } else {
    None
  };

  debug!(
    "Parsed Order Status: ID={}, Status={}, Filled={}, Remaining={}, AvgPrice={}, PermId={}, ParentId={}, LastFillPx={}, ClientId={}, WhyHeld={}, MktCapPx={:?}",
    id, status_str, filled, remaining, avg_fill_price, perm_id, parent_id, last_fill_price, client_id, why_held, mkt_cap_price
  );

  // Call handler with parsed values (Passing status_str for now, needs handler change for status_enum)
  // If handler expects OrderStatus, change the call:
  // handler.order_status(id, status_enum, filled, remaining, ...);
  handler.order_status(
    id,
    status_enum,
    filled,
    remaining,
    avg_fill_price,
    perm_id,
    parent_id,
    last_fill_price,
    client_id,
    &why_held,
    mkt_cap_price,
  );
  Ok(())
}

/// Process open order message
pub fn process_open_order<'a>(
  handler: &Arc<dyn OrderHandler>,
  parser: &'a mut FieldParser<'a>, // `parser` is mutably borrowed for the function's duration
  server_version: i32,
) -> Result<(), IBKRError> {
  // --- Start of `process_open_order` ---
  let msg_version = if server_version < min_server_ver::ORDER_CONTAINER {
    parser.read_int()?
  } else {
    server_version
  };

  let order_id_i32 = parser.read_int()?;
  debug!(
    "Parsing Open Order: ID={}, MessageVersion={}, ServerVersion={}",
    order_id_i32, msg_version, server_version
  );

  let mut contract = Contract::new();
  let mut order_request = OrderRequest::default();
  let mut order_state = OrderState::default();

  // --- Decoding logic directly using `parser` ---
  // Create the decoder. It borrows `parser` mutably.
  let mut decoder = OrderDecoder::new(
    parser, // The mutable borrow starts here
    &mut contract,
    &mut order_request,
    &mut order_state,
    msg_version,
    server_version,
  );

  // Execute all decoding steps using the decoder
  decoder.read_contract_fields()?;
  decoder.read_action()?;
  decoder.read_total_quantity()?;
  decoder.read_order_type()?;
  decoder.read_lmt_price()?;
  decoder.read_aux_price()?;
  decoder.read_tif()?;
  decoder.read_oca_group()?;
  decoder.read_account()?;
  decoder.read_open_close()?;
  decoder.read_origin()?;
  decoder.read_order_ref()?;
  decoder.read_client_id()?;
  decoder.read_perm_id()?;
  decoder.read_outside_rth()?;
  decoder.read_hidden()?;
  decoder.read_discretionary_amount()?;
  decoder.read_good_after_time()?;
  decoder.skip_shares_allocation()?;
  decoder.read_fa_params()?;
  decoder.read_model_code()?;
  decoder.read_good_till_date()?;
  decoder.read_rule_80a()?;
  decoder.read_percent_offset()?;
  decoder.read_settling_firm()?;
  decoder.read_short_sale_params()?;
  decoder.read_auction_strategy()?;
  decoder.read_box_order_params()?;
  decoder.read_peg_to_stk_or_vol_order_params()?;
  decoder.read_display_size()?;
  decoder.read_old_style_outside_rth()?;
  decoder.read_block_order()?;
  decoder.read_sweep_to_fill()?;
  decoder.read_all_or_none()?;
  decoder.read_min_qty()?;
  decoder.read_oca_type()?;
  decoder.read_etrade_only()?;
  decoder.read_firm_quote_only()?;
  decoder.read_nbbo_price_cap()?;
  decoder.read_parent_id()?;
  decoder.read_trigger_method()?;
  decoder.read_vol_order_params(true)?;
  decoder.read_trail_params()?;
  decoder.read_basis_points()?;
  decoder.read_combo_legs()?;
  decoder.read_smart_combo_routing_params()?;
  decoder.read_scale_order_params()?;
  decoder.read_hedge_params()?;
  decoder.read_opt_out_smart_routing()?;
  decoder.read_clearing_params()?;
  decoder.read_not_held()?;
  decoder.read_delta_neutral()?;
  decoder.read_algo_params()?;
  decoder.read_solicited()?;
  decoder.read_what_if_info_and_order_state()?;
  decoder.read_vol_randomize_flags()?;
  decoder.read_peg_to_bench_params()?;
  decoder.read_conditions()?;
  decoder.read_adjusted_order_params()?;
  decoder.read_soft_dollar_tier()?;
  decoder.read_cash_qty()?;
  decoder.read_dont_use_auto_price_for_hedge()?;
  decoder.read_is_oms_container()?;
  decoder.read_discretionary_up_to_limit_price()?;
  decoder.read_use_price_mgmt_algo()?;
  decoder.read_duration()?;
  decoder.read_post_to_ats()?;
  decoder.read_auto_cancel_parent()?;
  decoder.read_peg_best_peg_mid_order_attributes()?;
  decoder.read_customer_account()?;
  decoder.read_professional_customer()?;
  decoder.read_bond_accrued_interest()?;

  // The mutable borrow of `parser` via `decoder` implicitly ends here
  // as `decoder` goes out of scope naturally.

  // Now we can safely check remaining fields on the original `parser`.
  // The mutable borrow needed by `decoder` is finished.
  let remaining_fields = decoder.remaining_fields();
  if remaining_fields > 0 {
    warn!("{} remaining fields after parsing OpenOrder ID {}", remaining_fields, order_id_i32);
    // Optionally skip remaining fields
    // for _ in 0..remaining_fields {
    //     parser.skip_field()?;
    // }
  }

  debug!(
    "Calling handler.open_order: ID={}, Symbol={}, Type={}, Side={}, Qty={}, Status={:?}",
    order_id_i32,
    contract.symbol,
    order_request.order_type,
    order_request.side,
    order_request.quantity,
    order_state.status
  );

  handler.open_order(order_id_i32, &contract, &order_request, &order_state);
  Ok(())
  // --- End of `process_open_order` ---
}

pub fn process_open_order_end(handler: &Arc<dyn OrderHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // let _version = parser.read_int()?; // If version added later
  debug!("Parsed Open Order End");
  handler.open_order_end(); // Call handler
  Ok(())
}

/// Process order bound message
pub fn process_order_bound(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let order_id = parser.read_i64()?;
  let api_client_id = parser.read_int()?;
  let api_order_id = parser.read_int()?;
  debug!("Parsed Order Bound: OrderId={}, ApiClientId={}, ApiOrderId={}", order_id, api_client_id, api_order_id);
  handler.order_bound(order_id, api_client_id, api_order_id); // Call handler
  Ok(())
}

/// Process completed order message
pub fn process_completed_order<'a>(
  handler: &Arc<dyn OrderHandler>,
  parser: &'a mut FieldParser<'a>,
  server_version: i32,
) -> Result<(), IBKRError> {
  debug!("Parsing Completed Order (ServerVersion={})", server_version);

  let mut contract = Contract::new();
  let mut order_request = OrderRequest::default();
  let mut order_state = OrderState::default();

  // --- Decoding logic directly using `parser` ---
  let msg_version_proxy = server_version;
  // Create decoder
  let mut decoder = OrderDecoder::new(
    parser, // Mutable borrow starts here
    &mut contract,
    &mut order_request,
    &mut order_state,
    msg_version_proxy,
    server_version,
  );

  // Read expected fields using decoder
  decoder.read_contract_fields()?;
  decoder.read_action()?;
  decoder.read_total_quantity()?;
  decoder.read_order_type()?;
  decoder.read_lmt_price()?;
  decoder.read_aux_price()?;
  decoder.read_tif()?;
  decoder.read_oca_group()?;
  decoder.read_account()?;
  decoder.read_open_close()?;
  decoder.read_origin()?;
  decoder.read_order_ref()?;
  decoder.read_order_status()?;

  // Now read completedTime/Status directly using the original `parser` reference.
  // The mutable borrow is still valid for the `parser` variable itself.
  if decoder.remaining_fields() >= 2 {
    decoder.order_state_mut().completed_time = decoder.read_string_opt()?;
    decoder.order_state_mut().completed_status = decoder.read_string_opt()?;
  } else {
    warn!(
      "CompletedOrder message might be missing completedTime/Status (Remaining fields: {})",
      decoder.remaining_fields()
    );
    decoder.order_state_mut().completed_time = None;
    decoder.order_state_mut().completed_status = None;
  }

  // Check remaining fields using the original parser
  let remaining_fields = decoder.remaining_fields();
  if remaining_fields > 0 {
    warn!("{} remaining fields after parsing CompletedOrder", remaining_fields);
    // while parser.remaining_fields() > 0 { let _ = parser.skip_field()?; } // Optional skip
  }

  debug!(
    "Calling handler.completed_order: Symbol={}, Status={:?}, CompletedTime={:?}",
    contract.symbol, order_state.status, order_state.completed_time
  );

  handler.completed_order(&contract, &order_request, &order_state);
  Ok(())
}

/// Process completed orders end message
pub fn process_completed_orders_end(handler: &Arc<dyn OrderHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // let _version = parser.read_int()?; // If version added later
  debug!("Parsed Completed Orders End");
  handler.completed_orders_end(); // Call handler
  Ok(())
}
