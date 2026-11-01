// M4 W3 `dbshell` — C# / .NET 10 port over in-memory Microsoft.Data.Sqlite.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_154.md`.
// One statement per line; blanks and `--` comments are skipped; each statement
// is echoed as `sql> <statement>` and printed with its rendered result.
using System.Globalization;
using System.Text;
using Microsoft.Data.Sqlite;

namespace DbShell;

internal static class Program
{
    private const string Usage = "usage: dbshell <script>";

    private static void WriteLine(Stream stream, string text)
    {
        byte[] bytes = Encoding.UTF8.GetBytes(text + "\r\n");
        stream.Write(bytes, 0, bytes.Length);
        stream.Flush();
    }

    private static int Fail(string label)
    {
        WriteLine(Console.OpenStandardError(), "dbshell: " + label);
        return 1;
    }

    private static int FailFile(string label, string path)
    {
        WriteLine(Console.OpenStandardError(), "dbshell: " + label + path);
        return 2;
    }

    private static string Normalize(string message)
    {
        if (message.Contains("no such table", StringComparison.Ordinal))
        {
            return "no such table";
        }
        if (message.Contains("no such column", StringComparison.Ordinal))
        {
            return "unknown column in WHERE";
        }
        return message;
    }

    private static string Execute(SqliteConnection connection, string statement)
    {
        string kind = statement.Split(' ', 2)[0].ToUpperInvariant();
        using SqliteCommand command = connection.CreateCommand();
        command.CommandText = statement;

        if (kind == "SELECT")
        {
            if (statement.ToUpperInvariant().Contains("COUNT(", StringComparison.Ordinal))
            {
                object? value = command.ExecuteScalar();
                return value is null
                    ? "0"
                    : Convert.ToString(value, CultureInfo.InvariantCulture) ?? "0";
            }
            using SqliteDataReader reader = command.ExecuteReader();
            var builder = new StringBuilder();
            while (reader.Read())
            {
                for (int i = 0; i < reader.FieldCount; i++)
                {
                    if (i > 0)
                    {
                        builder.Append('\t');
                    }
                    builder.Append(reader.IsDBNull(i)
                        ? "NULL"
                        : Convert.ToString(reader.GetValue(i), CultureInfo.InvariantCulture));
                }
                builder.Append('\n');
            }
            return builder.ToString();
        }

        int affected = command.ExecuteNonQuery();
        if (kind is "UPDATE" or "DELETE")
        {
            return "OK " + affected.ToString(CultureInfo.InvariantCulture);
        }
        return "OK";
    }

    private static int Main(string[] args)
    {
        if (args.Any(arg => arg.StartsWith('-')))
        {
            return Fail("this tool has no options");
        }
        if (args.Length != 1)
        {
            return Fail(Usage);
        }

        string path = args[0];
        if (!File.Exists(path))
        {
            if (Directory.Exists(path))
            {
                return FailFile("not a regular file: ", path);
            }
            return FailFile("script does not exist: ", path);
        }

        string text;
        try
        {
            text = File.ReadAllText(path, Encoding.UTF8);
        }
        catch (IOException)
        {
            return FailFile("script does not exist: ", path);
        }

        using var connection = new SqliteConnection("Data Source=:memory:");
        connection.Open();

        Stream stdout = Console.OpenStandardOutput();
        int code = 0;
        foreach (string raw in text.Split('\n'))
        {
            string line = raw.Trim();
            if (line.Length == 0 || line.StartsWith("--", StringComparison.Ordinal))
            {
                continue;
            }
            WriteLine(stdout, "sql> " + line);
            string result;
            try
            {
                result = Execute(connection, line);
            }
            catch (SqliteException error)
            {
                result = "ERROR: " + Normalize(error.Message);
                code = 3;
            }
            WriteLine(stdout, result);
            if (code == 3)
            {
                break;
            }
        }
        return code;
    }
}
