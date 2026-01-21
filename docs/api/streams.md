# Streams

::: jce.stream.LengthPrefixedWriter
    options:
      members:
        - __init__
        - pack
        - get_buffer
        - clear
      show_root_toc_entry: false

::: jce.stream.LengthPrefixedReader
    options:
      members:
        - __init__
        - feed
        - __iter__
      show_root_toc_entry: false

::: jce.stream.JceStreamWriter
    options:
      members:
        - __init__
        - write
        - write_bytes
        - get_buffer
        - clear
      show_root_toc_entry: false

::: jce.stream.JceStreamReader
    options:
      members:
        - __init__
        - feed
        - has_packet
        - read_packet
      show_root_toc_entry: false
