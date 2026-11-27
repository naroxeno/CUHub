using System;
using System.IO;

namespace CuprumBridge
{
    public static class FrameCodec
    {
        public static byte[] Read(Stream stream)
        {
            if (stream == null) throw new ArgumentNullException(nameof(stream));
            var header = new byte[4];
            var filled = 0;
            while (filled < header.Length)
            {
                var count = stream.Read(header, filled, header.Length - filled);
                if (count == 0)
                {
                    if (filled == 0) return null;
                    throw new IOException("connection closed inside a frame header");
                }
                filled += count;
            }
            var length = (uint)(header[0] | header[1] << 8 | header[2] << 16 | header[3] << 24);
            if (length == 0) throw new InvalidDataException("frame length is zero");
            if (length > Protocol.MaxFrame)
                throw new InvalidDataException("frame header announces " + length + " bytes, limit is " + Protocol.MaxFrame);
            var body = new byte[length];
            filled = 0;
            while (filled < body.Length)
            {
                var count = stream.Read(body, filled, body.Length - filled);
                if (count == 0) throw new IOException("connection closed inside a frame body");
                filled += count;
            }
            return body;
        }

        public static void Write(Stream stream, byte[] payload)
        {
            if (stream == null) throw new ArgumentNullException(nameof(stream));
            if (payload == null) throw new ArgumentNullException(nameof(payload));
            if (payload.Length == 0) throw new InvalidDataException("refusing to write an empty frame");
            if (payload.Length > Protocol.MaxFrame)
                throw new InvalidDataException("frame is " + payload.Length + " bytes, limit is " + Protocol.MaxFrame);
            var frame = new byte[4 + payload.Length];
            frame[0] = (byte)payload.Length;
            frame[1] = (byte)(payload.Length >> 8);
            frame[2] = (byte)(payload.Length >> 16);
            frame[3] = (byte)(payload.Length >> 24);
            Buffer.BlockCopy(payload, 0, frame, 4, payload.Length);
            stream.Write(frame, 0, frame.Length);
            stream.Flush();
        }
    }
}
