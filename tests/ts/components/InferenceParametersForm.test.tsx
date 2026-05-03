import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import { InferenceParametersForm } from '../../../src/components/InferenceParametersForm';

describe('InferenceParametersForm stop sequences', () => {
  it('renders existing stop sequences as newline-delimited values', () => {
    const onChange = vi.fn();

    render(
      <InferenceParametersForm
        value={{ stop: ['<|im_end|>', '</s>'] }}
        onChange={onChange}
      />
    );

    const stopInput = screen.getByLabelText('Stop Sequences') as HTMLTextAreaElement;
    expect(stopInput.value).toBe('<|im_end|>\n</s>');
  });

  it('parses stop sequences from commas and newlines', () => {
    const onChange = vi.fn();

    render(
      <InferenceParametersForm
        value={{ temperature: 0.7 }}
        onChange={onChange}
      />
    );

    const stopInput = screen.getByLabelText('Stop Sequences');
    fireEvent.change(stopInput, {
      target: { value: ' <|im_end|>, </s>\n<|eot|> ' },
    });

    expect(onChange).toHaveBeenLastCalledWith({
      temperature: 0.7,
      stop: ['<|im_end|>', '</s>', '<|eot|>'],
    });
  });

  it('removes stop sequences via reset while preserving other values', () => {
    const onChange = vi.fn();

    render(
      <InferenceParametersForm
        value={{ topP: 0.92, stop: ['<|im_end|>'] }}
        onChange={onChange}
      />
    );

    fireEvent.click(screen.getByLabelText('Reset Stop Sequences to default'));

    expect(onChange).toHaveBeenCalledWith({ topP: 0.92 });
  });

  it('clears stop sequences when textarea becomes empty', () => {
    const onChange = vi.fn();

    render(
      <InferenceParametersForm
        value={{ repeatPenalty: 1.1, stop: ['<|im_end|>'] }}
        onChange={onChange}
      />
    );

    const stopInput = screen.getByLabelText('Stop Sequences');
    fireEvent.change(stopInput, { target: { value: '' } });

    expect(onChange).toHaveBeenLastCalledWith({ repeatPenalty: 1.1 });
  });
});
