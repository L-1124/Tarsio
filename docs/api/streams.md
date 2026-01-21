# Streams

::: jce.stream.LengthPrefixedWriter
    options:
      members:
        - __init__
        - pack
        - get_buffer
        - clear
      

::: jce.stream.LengthPrefixedReader
    options:
      members:
        - __init__
        - feed
        - __iter__
      

::: jce.stream.JceStreamWriter
    options:
      members:
        - __init__
        - write
        - write_bytes
        - get_buffer
        - clear
      

::: jce.stream.JceStreamReader
    options:
      members:
        - __init__
        - feed
        - has_packet
        - read_packet
      
