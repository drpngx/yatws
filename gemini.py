#!/bin/env python
import argparse
import glob
import json
import logging
import os
import sys
from typing import Optional

from google import genai
from google.genai import types


def MatchFiles(files: list[str]):
  inventory = glob.glob('yatws/**/*.rs')
  ret = []
  for f in files:
    for i in inventory:
      # We can match a suffix, prefix like parse, directory, or anything really.
      if f in i:
        ret.append(i)
  return ret


def ComponentFiles(comp):
  compfiles = {
    'base': [
      'yatws/src/account.rs',
      'yatws/src/base.rs',
      'yatws/src/handler.rs',
      'yatws/src/min_server_ver.rs',
      'yatws/src/conn.rs',
      'yatws/src/conn_log.rs',
      'yatws/src/conn_mock.rs',
      'yatws/src/order.rs',
      'yatws/src/order_builder.rs',
      'yatws/src/lib.rs',
      'yatws/src/contract.rs',
    ],
    'client': [
      'yatws/src/order_manager.rs',
      'yatws/src/news.rs',
      'yatws/src/data_manager.rs',
      'yatws/src/account_manager.rs',
      'yatws/src/data.rs',
      'yatws/src/client.rs'
    ],
    'tcp': [
      'yatws/src/protocol_encoder.rs',
      'yatws/src/protocol_decoder.rs',
      'yatws/src/protocol.rs',
    ],
    'parser': [
      'yatws/src/parser_client.rs',
      'yatws/src/message_parser.rs',
      'yatws/src/parser_data_market.rs',
      'yatws/src/protocol_dec_parser.rs',
      'yatws/src/parser_data_fin.rs',
      'yatws/src/parser_account.rs',
      'yatws/src/parser_order.rs',
      'yatws/src/parser_fin_adv.rs',
      'yatws/src/parser_data_ref.rs',
      'yatws/src/parser_data_news.rs',
    ],
  }
  if comp == 'all':
    comp = compfiles.keys()
  files = [f for c in comp for f in compfiles[c]]
  return files


# def DumpResp(resp):
#   logging.info('Usage: %s', resp.usage_metadata)
#   for c, cand in enumerate(resp.candidates):
#     logging.info('== Candidate[%d]', c)
#     for p, part in enumerate(cand.content.parts):
#       logging.info('Part[%d]:\n%s', p, part.text)


def PrintAvailableModels(client):
  for m in client.models.list():
    logging.info('Model: %s = %s', m.name, m.display_name)
    # logging.info('Model: %s = %s', m.name, m)

def RunListModels(client, _argv):
  PrintAvailableModels(client)


def _CheckCompFiles():
  files = ComponentFiles('all')
  found_files = glob.glob('yatws/**/*.rs')
  missing = []
  files = set(files)
  for f in found_files:
    if f not in files:
      missing.append(f)
  assert not missing, missing


def RunChat(client, argv):
  """Run chat without cache.

  The latest models don't support cacing, so the context files are specified in situ
  from files uploads.
  """
  def ParseArgs(argv):
    parser = argparse.ArgumentParser('Gemini.')
    # parser.add_argument('-c', '--components', type=str, nargs='+', help='Components to upload')
    parser.add_argument('-m', '--model', type=str, default='free', help='Model to use')
    return parser.parse_args(argv[1:])

  def GetTurn():
    lines = []
    prompt = 'Enter text followed by SEND: '
    files = []
    while True:
      line = input(prompt)
      if line.startswith('INCLUDE'):
        try:
          inc = ComponentFiles(line.split(' ')[1:])
          for f in inc:
            lines.append(f'Here is a source file: {f}');
            with open(f, 'rt', encoding='utf-8') as i:
              lines.append(i.read())
            print(f'Included {f}')
        except Exception as err:
          print(f'Fail: {err}')
      if line.startswith('FILE'):
        inc = MatchFiles(line.split(' ')[1:])
        for f in inc:
          lines.append(f'Here is a source file: {f}');
          with open(f, 'rt', encoding='utf-8') as i:
            lines.append(i.read())
          print(f'Included {f}')
      if line == 'SEND':
        break
      lines.append(line)
      prompt = ''
    return '\n'.join(lines), files

  args = ParseArgs(argv)
  # https://ai.google.dev/gemini-api/docs/pricing
  match args.model:
    case 'free':
      model = 'gemini-2.5-pro-exp-03-25'
    case _:
      model = 'gemini-2.5-pro-preview-03-25'

  history = []

  chat = client.chats.create(model=model, history=history)
  ctx_trail = ''
  while True:
    msg, inc = GetTurn()
    msg += ctx_trail
    logging.info('Sending message...')
    try:
      if not inc:
        response = chat.send_message(msg)
        print(response.text)
      else:
        fp = [types.Part.from_uri(mime_type=f.mime_type, file_uri=f.uri) for f in inc]
        text_part = types.Part.from_text(text=msg)
        response = chat.send_message(parts=fp + [text_part])
        ctx_trail = ''
        print(response.text)
    except Exception as err:
      logging.info('Ignoring failure: %s', err)


def RunListModels(client):
  models = client.models.list()

  for model in models:
    print(model.name)


def Run(argv):
  _CheckCompFiles()
  client = genai.Client(api_key=os.environ.get('GEMINI_API_KEY'))
  match argv[1]:
    case 'chat':
      RunChat(client, argv[1:])
    case 'list-models':
      RunListModels(client)
    case _:
      raise ValueError(f'Invalid command {argv[1]}')


if __name__ == '__main__':
  logging.getLogger().setLevel(logging.INFO)
  Run(sys.argv)
