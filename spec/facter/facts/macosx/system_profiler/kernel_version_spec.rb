# frozen_string_literal: true

describe 'Macosx SystemProfilerKernelVersion' do
  context '#call_the_resolver' do
    let(:value) { 'Darwin 18.7.0' }
    let(:expected_resolved_fact) { double(Facter::ResolvedFact, name: 'system_profiler.kernel_version', value: value) }
    subject(:fact) { Facter::Macosx::SystemProfilerKernelVersion.new }

    before do
      expect(Facter::Resolvers::SystemProfiler).to receive(:resolve).with(:kernel_version).and_return(value)
      expect(Facter::ResolvedFact).to receive(:new)
        .with('system_profiler.kernel_version', value)
        .and_return(expected_resolved_fact)
    end

    it 'returns system_profiler.kernel_version fact' do
      expect(fact.call_the_resolver).to eq(expected_resolved_fact)
    end
  end
end