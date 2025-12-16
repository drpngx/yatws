# YATWS (Yet Another TWS API)

A comprehensive, thread-safe, type-safe, and ergonomic Rust interface to the Interactive Brokers TWS API.

## Overview

YATWS provides a modern Rust implementation for interacting with Interactive Brokers' Trader Workstation (TWS) and Gateway. The library's manager-based architecture and support for both synchronous and asynchronous patterns make it suitable for various trading applications, from simple data retrieval to complex automated trading systems.

This library was born out of the need to place orders in rapid succession in response to market events, which is not easily accomplished with existing Rust crates. It takes about 3ms to place an order with this library. While order management was the primary focus, other interfaces (market data, account information, etc.) have been implemented for completeness.

**Current Status**: Early stage but US equity trading is production-ready. The API has been used to trade 9 figures of dollars in volume.

## Features

- **Comprehensive API Coverage**: Access to orders, accounts, market data, fundamentals, news, and reference data
- **Book-keeping**: Keeps the portfolio with PNL and order book up-to-date
- **Multiple Programming Patterns**:
  - Synchronous blocking calls with timeouts
  - Asynchronous observer pattern
  - Subscription model
- **Options Strategy Builder**: Simplified creation of common options strategies
- **Strong Type Safety**: Leverages Rust's type system for safer API interactions
- **Domain-Specific Managers**: Organized access to different API functionalities
- **Rate limits**: pace the requests in accordance with IBKR rules
- **Session Recording/Replay**: Record TWS interactions for testing and debugging

## Reading Further
For more information, see the [crate `README`](https://github.com/drpngx/yatws/yatws/README.md).
