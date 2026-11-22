import { useRef } from 'react';
import {
  Button,
  Checkbox,
  Group,
  NumberInput,
  PasswordInput,
  ScrollArea,
  Select,
  SimpleGrid,
  TextInput,
  Textarea,
} from '@mantine/core';
import { useForm } from '@mantine/form';
import { notifications } from '@mantine/notifications';
import { COUNTRIES } from '../../shared/data';

interface Values {
  firstName: string;
  lastName: string;
  email: string;
  phone: string;
  company: string;
  jobTitle: string;
  website: string;
  age: number | string;
  country: string | null;
  city: string;
  postalCode: string;
  password: string;
  confirmPassword: string;
  bio: string;
  acceptTerms: boolean;
}

const initialValues: Values = {
  firstName: '',
  lastName: '',
  email: '',
  phone: '',
  company: '',
  jobTitle: '',
  website: '',
  age: '',
  country: null,
  city: '',
  postalCode: '',
  password: '',
  confirmPassword: '',
  bio: '',
  acceptTerms: false,
};

const EMAIL = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;
const PHONE = /^[+0-9 ()-]{7,20}$/;
const POSTAL = /^[A-Za-z0-9 -]{3,10}$/;
const COUNTRY_OPTIONS = [...COUNTRIES];

const required = (v: string) => (v.trim() ? null : 'Required');

const validate = {
  firstName: required,
  lastName: required,
  email: (v: string) => (!v.trim() ? 'Required' : EMAIL.test(v) ? null : 'Invalid email'),
  phone: (v: string) => (!v || PHONE.test(v) ? null : 'Invalid phone number'),
  website: (v: string) =>
    !v || v.startsWith('http://') || v.startsWith('https://')
      ? null
      : 'Must start with http:// or https://',
  age: (v: number | string) => {
    if (v === '' || v === null) return 'Required';
    const n = Number(v);
    return Number.isInteger(n) && n >= 18 && n <= 120 ? null : 'Must be a whole number 18–120';
  },
  country: (v: string | null) => (v ? null : 'Required'),
  city: required,
  postalCode: (v: string) =>
    !v.trim() ? 'Required' : POSTAL.test(v) ? null : 'Invalid postal code',
  password: (v: string) =>
    v.length >= 8 && /[A-Za-z]/.test(v) && /[0-9]/.test(v)
      ? null
      : 'At least 8 characters with a letter and a digit',
  confirmPassword: (v: string, values: Values) =>
    v === values.password ? null : 'Passwords do not match',
  bio: (v: string) => (v.length <= 500 ? null : 'At most 500 characters'),
  acceptTerms: (v: boolean) => (v ? null : 'You must accept the terms'),
};

export function FormPage() {
  // Validate on submit, then live on every change.
  const submitted = useRef(false);
  const form = useForm<Values>({
    mode: 'controlled',
    initialValues,
    validate,
    onValuesChange: () => {
      if (submitted.current) form.validate();
    },
  });

  const onSubmit = form.onSubmit(
    () => {
      notifications.show({ title: 'Saved', message: 'The form is valid.', color: 'green' });
    },
    () => undefined,
  );

  return (
    <ScrollArea style={{ flex: 1, minHeight: 0 }} type="auto">
      <form
        onSubmit={(e) => {
          submitted.current = true;
          onSubmit(e);
        }}
        style={{ maxWidth: 820 }}
        noValidate
      >
        <SimpleGrid cols={2} spacing="md" verticalSpacing="sm">
          <TextInput label="First name" withAsterisk {...form.getInputProps('firstName')} />
          <TextInput label="Last name" withAsterisk {...form.getInputProps('lastName')} />
          <TextInput label="Email" withAsterisk {...form.getInputProps('email')} />
          <TextInput label="Phone" {...form.getInputProps('phone')} />
          <TextInput label="Company" {...form.getInputProps('company')} />
          <TextInput label="Job title" {...form.getInputProps('jobTitle')} />
          <TextInput label="Website" {...form.getInputProps('website')} />
          <NumberInput
            label="Age"
            withAsterisk
            allowDecimal={false}
            {...form.getInputProps('age')}
          />
          <Select
            label="Country"
            withAsterisk
            data={COUNTRY_OPTIONS}
            searchable
            {...form.getInputProps('country')}
          />
          <TextInput label="City" withAsterisk {...form.getInputProps('city')} />
          <TextInput label="Postal code" withAsterisk {...form.getInputProps('postalCode')} />
          <PasswordInput label="Password" withAsterisk {...form.getInputProps('password')} />
          <PasswordInput
            label="Confirm password"
            withAsterisk
            {...form.getInputProps('confirmPassword')}
          />
        </SimpleGrid>
        <Textarea
          mt="sm"
          label="Bio"
          autosize
          minRows={3}
          maxRows={6}
          {...form.getInputProps('bio')}
        />
        <Checkbox
          mt="md"
          label="I accept the terms"
          {...form.getInputProps('acceptTerms', { type: 'checkbox' })}
        />
        <Group mt="lg">
          <Button type="submit">Submit</Button>
          <Button
            variant="default"
            onClick={() => {
              submitted.current = false;
              form.reset();
            }}
          >
            Reset
          </Button>
        </Group>
      </form>
    </ScrollArea>
  );
}
