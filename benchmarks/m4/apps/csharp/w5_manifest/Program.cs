// M4 W5 `manifest` — C# / .NET 10 port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_156.md`.
//
// JSON config + recursive walk + SHA-256 + RFC 4180 CSV. Paths are relative
// to the config root with `/` separators and sorted ordinally.
using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace Manifest;

internal static class Program
{
    private const string Usage = "usage: manifest <config.json>";

    private record Config(string Root, string Out, string[] Extensions);

    private static void Out(string text)
    {
        Stream stdout = Console.OpenStandardOutput();
        byte[] bytes = Encoding.UTF8.GetBytes(text + "\r\n");
        stdout.Write(bytes, 0, bytes.Length);
        stdout.Flush();
    }

    private static int Fail(string label, int code)
    {
        Stream stderr = Console.OpenStandardError();
        byte[] bytes = Encoding.UTF8.GetBytes("manifest: " + label + "\r\n");
        stderr.Write(bytes, 0, bytes.Length);
        stderr.Flush();
        return code;
    }

    private static int FailFile(string label, string path, int code)
        => Fail(label + path, code);

    private static bool TryParseConfig(byte[] raw, out Config? config, out string message)
    {
        config = null;
        message = "";
        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(raw);
        }
        catch (JsonException)
        {
            message = "config is not valid JSON";
            return false;
        }

        using (document)
        {
            JsonElement root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
            {
                message = "config must be a JSON object";
                return false;
            }
            if (!root.TryGetProperty("root", out JsonElement rootValue)
                || !root.TryGetProperty("out", out JsonElement outValue)
                || !root.TryGetProperty("extensions", out JsonElement extValue))
            {
                message = "config needs root, out and extensions";
                return false;
            }
            if (rootValue.ValueKind != JsonValueKind.String
                || outValue.ValueKind != JsonValueKind.String
                || extValue.ValueKind != JsonValueKind.Array)
            {
                message = "config types must be string, string, array";
                return false;
            }
            var extensions = new List<string>();
            foreach (JsonElement item in extValue.EnumerateArray())
            {
                if (item.ValueKind != JsonValueKind.String)
                {
                    message = "config types must be string, string, array";
                    return false;
                }
                extensions.Add(item.GetString()!);
            }
            config = new Config(rootValue.GetString()!, outValue.GetString()!,
                extensions.ToArray());
            return true;
        }
    }

    private static bool Selected(string rel, string[] extensions)
    {
        string ext = Path.GetExtension(rel);
        if (ext.Length == 0)
        {
            return false;
        }
        return extensions.Any(wanted =>
            string.Equals(ext, wanted, StringComparison.OrdinalIgnoreCase));
    }

    private static string CsvField(string value)
    {
        if (value.IndexOfAny(new[] { ',', '"', '\r', '\n' }) < 0)
        {
            return value;
        }
        return "\"" + value.Replace("\"", "\"\"") + "\"";
    }

    private static int Main(string[] args)
    {
        if (args.Any(arg => arg.StartsWith('-')))
        {
            return Fail("this tool has no options", 1);
        }
        if (args.Length != 1)
        {
            return Fail(Usage, 1);
        }

        string path = args[0];
        if (!File.Exists(path))
        {
            if (Directory.Exists(path))
            {
                return FailFile("not a regular file: ", path, 2);
            }
            return FailFile("config does not exist: ", path, 2);
        }

        byte[] raw;
        try
        {
            raw = File.ReadAllBytes(path);
        }
        catch (IOException)
        {
            return FailFile("config does not exist: ", path, 2);
        }

        if (!TryParseConfig(raw, out Config? config, out string message))
        {
            return Fail(message, 3);
        }

        if (!Directory.Exists(config!.Root))
        {
            return FailFile("root does not exist: ", config.Root, 4);
        }

        var files = new List<string>();
        foreach (string full in Directory.EnumerateFiles(
                     config.Root, "*", SearchOption.AllDirectories))
        {
            string rel = Path.GetRelativePath(config.Root, full).Replace('\\', '/');
            if (Selected(rel, config.Extensions))
            {
                files.Add(rel);
            }
        }
        files.Sort(StringComparer.Ordinal);
        if (files.Count == 0)
        {
            return Fail("no file matched the extensions", 5);
        }

        var report = new StringBuilder();
        report.Append("path,size,sha256,ext\n");
        long totalBytes = 0;
        foreach (string rel in files)
        {
            string full = Path.Combine(config.Root, rel.Replace('/', Path.DirectorySeparatorChar));
            byte[] data = File.ReadAllBytes(full);
            totalBytes += data.Length;
            string digest = Convert.ToHexString(SHA256.HashData(data))
                .ToLowerInvariant()[..16];
            report.Append(CsvField(rel)).Append(',')
                .Append(data.Length.ToString(CultureInfo.InvariantCulture)).Append(',')
                .Append(digest).Append(',')
                .Append(CsvField(Path.GetExtension(rel))).Append('\n');
        }

        try
        {
            File.WriteAllBytes(config.Out, Encoding.UTF8.GetBytes(report.ToString()));
        }
        catch (IOException)
        {
            return FailFile("cannot write report: ", config.Out, 4);
        }
        catch (UnauthorizedAccessException)
        {
            return FailFile("cannot write report: ", config.Out, 4);
        }

        Out("files=" + files.Count.ToString(CultureInfo.InvariantCulture));
        Out("bytes=" + totalBytes.ToString(CultureInfo.InvariantCulture));
        Out("first=" + files[0]);
        return 0;
    }
}
