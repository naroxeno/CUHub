using System;
using System.Collections.Concurrent;
using System.IO;
using System.Net.Sockets;
using System.Text;
using System.Threading;

namespace CuprumBridge
{
    public sealed class BridgeClient
    {
        static readonly int[] BackoffMilliseconds = { 1000, 2000, 5000, 10000, 30000 };
        const int ConnectTimeoutMilliseconds = 2000;

        readonly string _host;
        readonly int _port;
        readonly Action<string> _log;
        readonly ConcurrentQueue<BridgeMessage> _incoming = new ConcurrentQueue<BridgeMessage>();
        readonly ConcurrentQueue<BridgeMessage> _outgoing = new ConcurrentQueue<BridgeMessage>();
        readonly AutoResetEvent _writeSignal = new AutoResetEvent(false);
        readonly object _sync = new object();
        Thread _readThread;
        Thread _writeThread;
        TcpClient _client;
        NetworkStream _stream;
        volatile bool _running;
        volatile bool _connected;
        bool _started;
        int _backoffIndex;
        bool _loggedFailure;
        bool _loggedOversize;
        string _lastConnectError;

        public BridgeClient(string host, int port, Action<string> log)
        {
            if (string.IsNullOrEmpty(host)) throw new ArgumentException("host is required", nameof(host));
            if (port <= 0 || port > 65535) throw new ArgumentOutOfRangeException(nameof(port));
            _host = host;
            _port = port;
            _log = log ?? (message => { });
        }

        public bool IsConnected => _connected;

        public void Start()
        {
            if (_started) throw new InvalidOperationException("BridgeClient instances are single-use");
            _started = true;
            _running = true;
            _readThread = new Thread(ReadLoop) { IsBackground = true, Name = "CuprumBridge.Read" };
            _writeThread = new Thread(WriteLoop) { IsBackground = true, Name = "CuprumBridge.Write" };
            _readThread.Start();
            _writeThread.Start();
        }

        public void Stop()
        {
            if (!_running) return;
            _running = false;
            CloseConnection();
            _writeSignal.Set();
        }

        public void Send(BridgeMessage message)
        {
            if (message == null) return;
            _outgoing.Enqueue(message);
            _writeSignal.Set();
        }

        public bool TryReceive(out BridgeMessage message) => _incoming.TryDequeue(out message);

        void ReadLoop()
        {
            while (_running)
            {
                if (!_connected)
                {
                    if (!TryConnect())
                    {
                        LogFailure("connect to " + _host + ":" + _port + " failed: " + (_lastConnectError ?? "unknown error"));
                        WaitBackoff();
                        continue;
                    }
                }
                try
                {
                    var payload = FrameCodec.Read(_stream);
                    if (payload == null) throw new IOException("host closed the connection");
                    if (BridgeMessage.TryParse(Encoding.UTF8.GetString(payload), out var message, out _))
                    {
                        _incoming.Enqueue(message);
                    }
                    else
                    {
                        LogFailure("dropped a malformed bridge message");
                    }
                }
                catch (Exception ex)
                {
                    LogFailure("connection lost: " + ex.Message);
                    CloseConnection();
                    WaitBackoff();
                }
            }
        }

        void WriteLoop()
        {
            while (_running)
            {
                _writeSignal.WaitOne(500);
                if (!_running) break;
                while (_outgoing.TryDequeue(out var message))
                {
                    var stream = _stream;
                    if (stream == null || !_connected) continue;
                    byte[] payload;
                    try
                    {
                        payload = Encoding.UTF8.GetBytes(message.ToJson());
                    }
                    catch (Exception ex)
                    {
                        LogFailure("could not encode a bridge message: " + ex.Message);
                        continue;
                    }
                    if (payload.Length > Protocol.MaxFrame)
                    {
                        if (!_loggedOversize)
                        {
                            _loggedOversize = true;
                            _log("bridge message of " + payload.Length + " bytes exceeds the frame limit");
                        }
                        continue;
                    }
                    try
                    {
                        FrameCodec.Write(stream, payload);
                    }
                    catch (Exception ex)
                    {
                        LogFailure("write failed: " + ex.Message);
                        CloseConnection();
                        break;
                    }
                }
            }
        }

        bool TryConnect()
        {
            TcpClient client = null;
            try
            {
                client = new TcpClient { NoDelay = true };
                var result = client.BeginConnect(_host, _port, null, null);
                if (!result.AsyncWaitHandle.WaitOne(ConnectTimeoutMilliseconds))
                    throw new TimeoutException("connect timed out");
                client.EndConnect(result);
                var stream = client.GetStream();
                lock (_sync)
                {
                    if (!_running)
                    {
                        client.Close();
                        return false;
                    }
                    _client = client;
                    _stream = stream;
                    _connected = true;
                }
                _backoffIndex = 0;
                _loggedFailure = false;
                _loggedOversize = false;
                return true;
            }
            catch (Exception ex)
            {
                _lastConnectError = ex.Message;
                try
                {
                    client?.Close();
                }
                catch (Exception)
                {
                }
                return false;
            }
        }

        void CloseConnection()
        {
            lock (_sync)
            {
                _connected = false;
                var stream = _stream;
                var client = _client;
                _stream = null;
                _client = null;
                try
                {
                    stream?.Dispose();
                }
                catch (Exception)
                {
                }
                try
                {
                    client?.Close();
                }
                catch (Exception)
                {
                }
            }
        }

        void WaitBackoff()
        {
            var index = _backoffIndex < BackoffMilliseconds.Length ? _backoffIndex : BackoffMilliseconds.Length - 1;
            var delay = BackoffMilliseconds[index];
            if (_backoffIndex < BackoffMilliseconds.Length - 1) _backoffIndex++;
            var waited = 0;
            while (_running && waited < delay)
            {
                var slice = Math.Min(100, delay - waited);
                Thread.Sleep(slice);
                waited += slice;
            }
        }

        void LogFailure(string message)
        {
            if (_loggedFailure) return;
            _loggedFailure = true;
            _log(message);
        }
    }
}
