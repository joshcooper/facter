# frozen_string_literal: true

describe Facter::FactFilter do
  subject(:fact_filter) { Facter::FactFilter.new(options: options) }

  let(:options) { Facter::Options.get }

  describe '#filter_facts!' do
    context 'when legacy facts are blocked' do
      let(:fact_value) { 'value_1' }
      let(:resolved_fact) { Facter::ResolvedFact.new('my_fact', fact_value, :legacy) }

      before do
        allow(options).to receive(:[])
        allow(options).to receive(:[]).with(:show_legacy).and_return(false)
      end

      it 'filters blocked legacy facts' do
        fact_filter_input = [resolved_fact]
        fact_filter.filter_facts!(fact_filter_input, [])
        expect(fact_filter_input).to eq([])
      end

      context 'when user_query is provided' do
        it 'does not filter out the requested fact' do
          fact_filter_input = [resolved_fact]
          result = fact_filter.filter_facts!([resolved_fact], ['my_fact'])
          expect(result).to eql(fact_filter_input)
        end
      end
    end
  end
end
