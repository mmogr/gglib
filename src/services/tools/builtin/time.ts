/**
 * Built-in tool: get_current_time
 * Returns the current date and time in various formats.
 */

import type { ToolDefinition, ToolExecutor, JSONSchema } from '../types';

const parameters: JSONSchema = {
  type: 'object',
  properties: {
    timezone: {
      type: 'string',
      description:
        'IANA timezone name (e.g., "America/New_York", "Europe/London", "Asia/Tokyo"). Defaults to local timezone if not specified.',
    },
    format: {
      type: 'string',
      description: 'Output format: "iso" for ISO 8601, "human" for human-readable, "unix" for Unix timestamp. Defaults to "human".',
      enum: ['iso', 'human', 'unix'],
      default: 'human',
    },
  },
  required: [],
};

export const definition: ToolDefinition = {
  type: 'function',
  function: {
    name: 'get_current_time',
    description:
      'Get the current date and time. Can return time in different timezones and formats. Useful for time-sensitive queries or scheduling.',
    parameters,
  },
};

interface TimeArgs {
  timezone?: string;
  format?: 'iso' | 'human' | 'unix';
}

export const execute: ToolExecutor = (args) => {
  const { timezone, format = 'human' } = args as TimeArgs;

  try {
    const now = new Date();

    // Format options for Intl.DateTimeFormat
    const formatOptions: Intl.DateTimeFormatOptions = {
      timeZone: timezone,
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      timeZoneName: 'short',
    };

    let result: { time: string | number; timezone: string; format: string };

    switch (format) {
      case 'iso':
        // For ISO format with timezone, we need to handle it differently
        if (timezone) {
          const formatter = new Intl.DateTimeFormat('en-CA', {
            timeZone: timezone,
            year: 'numeric',
            month: '2-digit',
            day: '2-digit',
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
            hour12: false,
          });
          const parts = formatter.formatToParts(now);
          const get = (type: string) =>
            parts.find((p) => p.type === type)?.value || '';
          result = {
            time: `${get('year')}-${get('month')}-${get('day')}T${get('hour')}:${get('minute')}:${get('second')}`,
            timezone: timezone,
            format: 'iso',
          };
        } else {
          result = {
            time: now.toISOString(),
            timezone: 'UTC',
            format: 'iso',
          };
        }
        break;

      case 'unix':
        result = {
          time: Math.floor(now.getTime() / 1000),
          timezone: 'UTC',
          format: 'unix',
        };
        break;

      case 'human':
      default:
        const humanFormatter = new Intl.DateTimeFormat('en-US', formatOptions);
        result = {
          time: humanFormatter.format(now),
          timezone:
            timezone ||
            Intl.DateTimeFormat().resolvedOptions().timeZone ||
            'local',
          format: 'human',
        };
        break;
    }

    return { success: true, data: result };
  } catch (err) {
    // Handle invalid timezone
    if (err instanceof RangeError) {
      return {
        success: false,
        error: `Invalid timezone: "${timezone}". Use IANA timezone names like "America/New_York" or "Europe/London".`,
      };
    }
    return {
      success: false,
      error: err instanceof Error ? err.message : String(err),
    };
  }
};
