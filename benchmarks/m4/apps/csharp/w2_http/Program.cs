// M4 W2 `netcheck` — C# / .NET 10 port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_153.md`.
//
//     netcheck <host> <port> <path>
//
// Transport failures and completed responses without a status line share
// exit 2 and one stable message. A 2xx body must be a JSON object with an
// integral `value`.
using System.Globalization;
using System.Text;
using System.Text.Json;

namespace NetCheck;

internal static class Program
{
    private const string Usage = "usage: netcheck <host> <port> <path>";

    private static void WriteLine(Stream stream, string text)
    {
        byte[] bytes = Encoding.UTF8.GetBytes(text + "\r\n");
        stream.Write(bytes, 0, bytes.Length);
        stream.Flush();
    }

    private static int Fail(string label, int code)
    {
        WriteLine(Console.OpenStandardError(), "netcheck: " + label);
        return code;
    }

    private static async Task<int> Main(string[] args)
    {
        if (args.Any(arg => arg.StartsWith('-')))
        {
            return Fail("this tool has no options", 1);
        }
        if (args.Length != 3)
        {
            return Fail(Usage, 1);
        }
        if (!int.TryParse(args[1], NumberStyles.None, CultureInfo.InvariantCulture,
                out int port) || port < 1 || port > 65535)
        {
            return Fail(Usage, 1);
        }

        string url = "http://" + args[0] + ":" + port.ToString(CultureInfo.InvariantCulture)
                     + args[2];

        using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(10) };
        int status;
        byte[] body;
        try
        {
            using HttpResponseMessage response = await client.GetAsync(url);
            status = (int)response.StatusCode;
            body = await response.Content.ReadAsByteArrayAsync();
        }
        catch (Exception error) when (error is HttpRequestException or TaskCanceledException)
        {
            return Fail("connection failed", 2);
        }

        bool parsed = true;
        long? value = null;
        try
        {
            using JsonDocument document = JsonDocument.Parse(body);
            JsonElement root = document.RootElement;
            if (root.ValueKind == JsonValueKind.Object
                && root.TryGetProperty("value", out JsonElement member)
                && member.ValueKind == JsonValueKind.Number
                && member.TryGetInt64(out long number))
            {
                value = number;
            }
        }
        catch (JsonException)
        {
            parsed = false;
        }

        if (status < 400)
        {
            if (!parsed)
            {
                return Fail("malformed JSON", 3);
            }
            if (value is null)
            {
                return Fail("missing value", 3);
            }
        }

        Stream stdout = Console.OpenStandardOutput();
        WriteLine(stdout, "STATUS " + status.ToString(CultureInfo.InvariantCulture));
        WriteLine(stdout, "BODY " + body.Length.ToString(CultureInfo.InvariantCulture));
        if (value is not null)
        {
            WriteLine(stdout, "VALUE " + value.Value.ToString(CultureInfo.InvariantCulture));
        }
        return 0;
    }
}
